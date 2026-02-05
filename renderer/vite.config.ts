import { defineConfig } from 'vite';

export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  server: {
    port: parseInt(process.env.VITE_PORT || '3000'),
    open: false,
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
});
