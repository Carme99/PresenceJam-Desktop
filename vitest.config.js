import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

// #443: minimal frontend harness — jsdom environment (LogViewer mounts
// a real component for #492; the theme listener dispatches real
// StorageEvents for #423), $lib alias to src/lib (mirrors the
// SvelteKit alias in .svelte-kit/tsconfig.json), plus the svelte
// plugin so .svelte(.ts) store modules compile.
// TWO npm scripts: `npm test` -> `vitest run` (the fast inner loop) and
// `npm run test:coverage` -> the same run plus the v8 coverage gate. CI's
// `frontend` job runs the coverage variant, so a coverage regression fails the
// job the same way a broken test does.
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
    include: ['tests/**/*.test.ts'],
    // #681: the coverage RATCHET. The four thresholds below are the values
    // MEASURED on the merged 4.7.0 tree (origin/main @ 912e6b6), not an
    // aspiration: raise one only in the same PR that raises the measured
    // number, and never above what `npm run test:coverage` prints. A drop
    // fails the run (and so CI's `frontend` job), which is the whole point —
    // a threshold that cannot fail is not a ratchet.
    //
    // `include` is the whole source set the harness can reach, so the ratchet
    // cannot drift by measuring a narrower tree than the baseline was measured
    // over. Vitest compares istanbul's *floored* percentage against these, so a
    // value copied from the printed summary can never fail on rounding.
    coverage: {
      provider: 'v8',
      include: ['src/**', 'tests/**'],
      reporter: ['text', 'lcov'],
      thresholds: {
        statements: 65.81,
        branches: 57.39,
        functions: 70.7,
        lines: 64.51
      }
    }
  }
});
