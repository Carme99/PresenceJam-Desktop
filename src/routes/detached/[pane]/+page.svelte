<script lang="ts">
  import { page } from '$app/state';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import { t } from '$lib/i18n';
  // C7: entry point for detached windows. Created from the main window
  // via WebviewWindow with url `/detached/<pane>` (SPA fallback serves
  // index.html; the SvelteKit router hydrates this route). The root
  // +layout.svelte guards its main-window-only listeners by window label,
  // so no reconnect/update handlers duplicate here.
</script>

{#if page.params.pane === 'logs'}
  <LogViewer detached />
{:else if page.params.pane === 'settings'}
  <Settings detached />
{:else}
  <div class="unknown">{t('routes.unknownPane', { pane: page.params.pane ?? '' })}</div>
{/if}

<style>
  .unknown {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--fg-muted);
  }
</style>
