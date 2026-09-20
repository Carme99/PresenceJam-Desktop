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
    // Vitest 4 defaults to the `forks` pool, which evaluates test modules via
    // Vite's ModuleRunner in a separate VM context. The jsdom environment's
    // `setup(globalThis)` populates the worker process's `globalThis`, but
    // that VM context does not see those globals, so a test file that reads
    // `window.localStorage` at module-init time (e.g. `src/lib/stores/theme.ts`
    // hydrating from localStorage) crashes before any test runs. The
    // `vmThreads` pool runs the module evaluator in a context that the env
    // setup has already configured, so the jsdom globals are visible where
    // the test modules actually execute.
    pool: 'vmThreads',
    include: ['tests/**/*.test.ts'],
    // #681: the coverage RATCHET. The four thresholds below are the values
    // MEASURED on the tree this branch merges into (rebased onto
    // origin/main @ 8a9f79d), not an aspiration: raise one only in the same PR
    // that raises the measured number, and never above what
    // `npm run test:coverage` prints. A drop fails the run (and so CI's
    // `frontend` job), which is the whole point — a threshold that cannot fail
    // is not a ratchet. Vitest compares istanbul's *floored* percentage against
    // these, so a value copied from the printed summary cannot fail on rounding.
    coverage: {
      provider: 'v8',
      // `tests/**` is inert: vitest's default `coverage.exclude` already drops
      // `**/*.test.ts`. Kept so the intended set is stated rather than implied;
      // the set actually measured is `src/**` minus the deliberate exclusions
      // below.
      include: ['src/**', 'tests/**'],
      // Excluded ON PURPOSE, and stated here rather than silently dropped:
      //  - `src/lib/types-generated/**` is ts-rs output — derived, gitignored,
      //    hence absent on a fresh checkout but materialised by CI's
      //    `cargo test --lib` step before this command runs. Excluding it keeps
      //    the local and CI denominators identical. (It currently contributes
      //    zero statements, but a future generated const/enum would not, and
      //    then the two environments would silently measure different trees.)
      //  - `src/app.html` and the detached-pane route are not instrumentable by
      //    the v8 provider: rollup cannot parse them while remapping uncovered
      //    files, and vitest drops them with "Failed to parse … Excluding it
      //    from coverage". Listing them keeps the denominator stable instead of
      //    letting them re-enter the set — and fail the gate spuriously — if a
      //    future change makes them parseable.
      exclude: [
        'src/lib/types-generated/**',
        'src/app.html',
        'src/routes/detached/**'
      ],
      reporter: ['text', 'lcov'],
      thresholds: {
        statements: 68.91,
        branches: 62.57,
        functions: 73.33,
        lines: 68.02
      }
    }
  }
});
