/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const backend = process.env.WEFT_BACKEND ?? 'http://localhost:8080';
// Sign-in redirects back to http://127.0.0.1:3000/callback, the URI registered on the Logto app.
const host = process.env.WEFT_CLIENT_HOST ?? '127.0.0.1';
const port = Number(process.env.WEFT_CLIENT_PORT ?? 3000);

export default defineConfig({
  plugins: [react()],
  server: {
    host,
    port,
    strictPort: true,
    // The app is same-origin with this server: no other site may read its responses (including the /api proxy).
    cors: false,
    proxy: {
      '/api': backend,
      '/hooks': backend,
      '/oauth': backend,
      '/healthz': backend,
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    globals: true,
    css: false,
  },
});
