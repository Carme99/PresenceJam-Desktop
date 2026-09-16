import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

// #443: minimal frontend harness — jsdom environment (LogViewer mounts
// a real component for #492; the theme listener dispatches real
// StorageEvents for #423), $lib alias to src/lib (mirrors the
// SvelteKit alias in .svelte-kit/tsconfig.json), plus the svelte
// plugin so .svelte(.ts) store modules compile.
// ONE npm script: `npm test` -> `vitest run`.
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url))
    },
    // Svelte ships server/client entry points; the browser condition
    // selects index-client so mount()/lifecycle work under jsdom.
    conditions: ['browser']
  },
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.test.ts']
  }
});
