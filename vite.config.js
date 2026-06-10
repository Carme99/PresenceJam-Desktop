import pkg from './package.json' with { type: 'json' };
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  define: {
    // Injected at build time — version from package.json + ISO build date.
    // Rendered in the About panel and the main page footer as e.g.
    // "2.6.1 (2026-06-09)". Replaced the previous "version.unix-ms"
    // format which produced an unreadable "2.6.0.1749350400000" in
    // the UI (PR-time code review flagged the upstream smell).
    //
    // Use the path-based key `import.meta.env.VITE_APP_BUILD` so esbuild's
    // define plugin matches the member-expression access in the consumer
    // (the bare-token `__APP_BUILD__` define only matches top-level
    // identifier uses, not the `import.meta.env.__APP_BUILD__` member
    // expression the Svelte consumer actually reads).
    "import.meta.env.VITE_APP_BUILD": JSON.stringify(
      `${pkg.version} (${new Date().toISOString().slice(0, 10)})`
    ),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
