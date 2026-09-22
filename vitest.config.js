import { mergeConfig } from 'vite';
import { defineConfig } from 'vitest/config';
import appConfig from './vite.config.js';

// #443: frontend harness — jsdom environment (LogViewer mounts a real
// component for #492; the theme listener dispatches real StorageEvents for
// #423), plus the four coverage thresholds of the #681 ratchet.
//
// #839: this file takes precedence over vite.config.js for the test run, and it
// used to re-declare by hand everything the tests needed — the svelte plugin,
// and a `$lib` alias mirrored from .svelte-kit/tsconfig.json — so it inherited
// neither the app's `define` block nor SvelteKit's plugin. Two consequences,
// both closed here: `import.meta.env.VITE_APP_BUILD` (defined in
// vite.config.js, rendered by About.svelte and the dashboard footer) was
// `undefined` under test, so a build-define regression (#63) could not be
// caught and any test asserting on the rendered version string failed against
// correct code; and `$app/*` / `$env/*` did not resolve at all, so no
// SvelteKit-dependent component could be imported by a test (the detached-pane
// route imports `$app/state`).
//
// The app config is DERIVED from, not copied: the define block, the plugin set
// and the `$lib` alias now have one definition, so they cannot drift apart.
// Two mechanics worth knowing:
//  - vite.config.js exports a callback (`defineConfig(async () => ({ … }))`)
//    and vite's `mergeConfig` refuses callback configs outright ("Cannot merge
//    config in form of callback"), so this file is an async callback too: it
//    resolves the app config first (with the same ConfigEnv vitest passes us,
//    so a future env-dependent app config behaves as it does under
//    `vite build`), then merges the test-only block over the result.
//  - the hand-written `$lib` alias is gone on purpose: SvelteKit's plugin
//    resolves `$lib` from `svelte.config.js` (`files.lib`, the same src/lib),
//    and it was the hand-written copy that could drift from that mapping.
//
// TWO npm scripts: `npm test` -> `vitest run` (the fast inner loop) and
// `npm run test:coverage` -> the same run plus the v8 coverage gate. CI's
// `frontend` job runs the coverage variant, so a coverage regression fails the
// job the same way a broken test does.
export default defineConfig(async (env) => {
  const appViteConfig = await appConfig(env);

  return mergeConfig(
    appViteConfig,
    defineConfig({
      resolve: {
        // Svelte ships server/client entry points; the browser condition
        // selects index-client so mount()/lifecycle work under jsdom.
        conditions: ['browser']
      },
      test: {
        environment: 'jsdom',
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
    })
  );
});
