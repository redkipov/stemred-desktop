import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version || 'unknown'),
  },
  server: {
    strictPort: true,
    host: '127.0.0.1',
    port: 1420,
  },
});
