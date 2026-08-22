<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { currentView, type View } from '$lib/stores/app';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Dashboard from '$lib/components/Dashboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import Diagnostics from '$lib/components/Diagnostics.svelte';
  import { devLog } from '$lib/utils/dev';
  import About from '$lib/components/About.svelte';
  import Reconnect from '$lib/components/Reconnect.svelte';
  import { t } from '$lib/i18n';

  // Build info — injected at build time via vite.config.js define
  // (mirrors the consumer in About.svelte; the vite define key is the
  // path-based `import.meta.env.VITE_APP_BUILD`, not the bare token
  // `__APP_BUILD__` that esbuild's define plugin can't match against a
  // member expression).
  const BUILD = import.meta.env.VITE_APP_BUILD ?? 'dev build';

  let ready = $state(false);
  let unlisten: (() => void)[] = [];

  onMount(() => {
    devLog('[PAGE] onMount: ENTRY');
    let unlistenTray: (() => void) | undefined;
    let unlistenShutdown: (() => void) | undefined;

    (async () => {
      devLog('[PAGE] onMount: calling invoke is_onboarding_complete');
      try {
        const complete = await invoke<boolean>('is_onboarding_complete');
        devLog('[PAGE] onMount: is_onboarding_complete SUCCESS, complete=', complete);
        currentView.set(complete ? 'dashboard' : 'onboarding');
        devLog('[PAGE] onMount: currentView set to:', complete ? 'dashboard' : 'onboarding');
      } catch (e) {
        console.error('[PAGE] onMount: is_onboarding_complete FAILED:', e);
        currentView.set('onboarding');
        devLog('[PAGE] onMount: currentView set to onboarding (from error)');
      }
      ready = true;
      devLog('[PAGE] onMount: ready=true');
    })();

    devLog('[PAGE] onMount: setting up tray-click listener');
    let destroyed = false;
    listen('tray-click', async () => {
      devLog('[PAGE] EVENT: tray-click received');
      devLog('[PAGE] EVENT: calling invoke show_window');
      try {
        await invoke('show_window');
      } catch (e) {
        console.warn('[PAGE] show_window failed:', e);
      }
    }).then(fn => {
      if (destroyed) fn();
      else unlistenTray = fn;
      devLog('[PAGE] onMount: tray-click listener registered');
    });
    devLog('[PAGE] onMount: setting up app-shutdown listener');
    listen('app-shutdown', async () => {
      devLog('[PAGE] EVENT: app-shutdown received');
      try {
        await invoke('app_exit');
        devLog('[PAGE] EVENT: app_exit SUCCESS');
      } catch (e) {
        console.error('[PAGE] EVENT: app_exit FAILED:', e);
      }
    }).then(fn => {
      if (destroyed) fn();
      else unlistenShutdown = fn;
      devLog('[PAGE] onMount: app-shutdown listener registered');
    });

    devLog('[PAGE] onMount: setting up navigate listener');
    listen<string>('navigate', (event) => {
      devLog('[PAGE] EVENT: navigate received:', event.payload);
      // C2: deep-link auth completions also emit 'navigate'. While the
      // Onboarding view is up it owns its own phase transitions — jumping
      // to another view would strand setup half-done — so programmatic
      // navigation is ignored until onboarding yields the view.
      if (!ready || $currentView === 'onboarding') return;
      currentView.set(event.payload as View);
    }).then(fn => { if (destroyed) fn(); else unlisten.push(fn); });

    devLog('[PAGE] onMount: setting up open-logs-folder listener');
    listen('open-logs-folder', async () => {
      devLog('[PAGE] EVENT: open-logs-folder received');
      try {
        await invoke('open_logs_folder');
      } catch (e) {
        console.error('[PAGE] EVENT: open_logs_folder FAILED:', e);
      }
    }).then(fn => { if (destroyed) fn(); else unlisten.push(fn); });

    devLog('[PAGE] onMount: setting up show-about listener');
    listen('show-about', () => {
      devLog('[PAGE] EVENT: show-about received');
      currentView.set('about');
    }).then(fn => { if (destroyed) fn(); else unlisten.push(fn); });

    devLog('[PAGE] onMount: setting up toggle-pause listener');
    listen('toggle-pause', async () => {
      devLog('[PAGE] EVENT: toggle-pause received');
      try {
        const status = await invoke<{ is_syncing: boolean }>('get_sync_status');
        if (status.is_syncing) {
          devLog('[PAGE] EVENT: calling invoke stop_syncing');
          await invoke('stop_syncing');
        } else {
          devLog('[PAGE] EVENT: calling invoke start_syncing');
          await invoke('start_syncing');
        }
      } catch (e) {
        console.warn('[PAGE] toggle-pause failed:', e);
      }
    }).then(fn => { if (destroyed) fn(); else unlisten.push(fn); });

    return () => {
      destroyed = true;
      devLog('[PAGE] onDestroy: ENTRY');
      if (unlistenTray) {
        unlistenTray();
        devLog('[PAGE] onDestroy: tray-click listener removed');
      }
      if (unlistenShutdown) {
        unlistenShutdown();
        devLog('[PAGE] onDestroy: app-shutdown listener removed');
      }
      unlisten.forEach(fn => fn());
      unlisten = [];
      devLog('[PAGE] onDestroy: all listeners cleaned up');
      devLog('[PAGE] onDestroy: EXIT');
    };
  });

  devLog('[PAGE] currentView value:', $currentView);
</script>

{#if !ready}
  <div class="loading">
    <span>{t('common.loading')}</span>
  </div>
{:else}
  <div class="app-container" id="main-content" tabindex="-1">
    {#if $currentView === 'onboarding'}
      <Onboarding />
    {:else if $currentView === 'dashboard'}
      <Dashboard />
    {:else if $currentView === 'settings'}
      <Settings />
    {:else if $currentView === 'logs'}
      <LogViewer />
    {:else if $currentView === 'diagnostics'}
      <Diagnostics />
    {:else if $currentView === 'about'}
      <About />
    {:else if $currentView === 'reconnect'}
      <Reconnect />
    {/if}
    <div class="version">{BUILD}</div>
  </div>
{/if}

<style>
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--bg-base);
    color: var(--fg-muted);
    font-size: 16px;
  }
  .app-container {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .version {
    position: fixed;
    bottom: 8px;
    right: 12px;
    font-size: 11px;
    color: var(--fg-subtle);
    opacity: 0.6;
    pointer-events: none;
  }
</style>
