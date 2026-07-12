<script>
  import '../app.css';
  // Side-effect import — installs the module-level subscribe that
  // applies the persisted theme (and keeps it in sync with future
  // changes). Without this, theme only applies when Settings mounts.
  import '$lib/stores/theme';
  import { devLog } from '$lib/utils/dev';
  devLog(`[LAYOUT] PresenceJam build: ${import.meta.env.VITE_APP_BUILD ?? 'dev build'}`);
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href="/icon.svg" />
  <link rel="alternate icon" type="image/png" href="/favicon.png" />
  <meta name="color-scheme" content="dark light" />
</svelte:head>

<slot />
