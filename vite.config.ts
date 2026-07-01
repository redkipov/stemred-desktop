import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  clearScreen: false,
  server: {
    strictPort: true,
    host: '127.0.0.1',
    port: 1420,
  },
});
