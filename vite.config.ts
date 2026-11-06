import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri expects a fixed port in dev and serves the built files from dist/ in release.
export default defineConfig({
  root: 'ui',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: ['es2021', 'safari15'],
    reportCompressedSize: true,
  },
  test: {
    root: '.',
    include: ['ui/**/*.test.ts'],
  },
});
