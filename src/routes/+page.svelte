<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { currentView, type View } from '$lib/stores/app';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Dashboard from '$lib/components/Dashboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import { devLog } from '$lib/utils/dev';
  import About from '$lib/components/About.svelte';
  import Reconnect from '$lib/components/Reconnect.svelte';

  // Build info — injected at build time via vite.config.js define
  const BUILD = import.meta.env.__APP_BUILD__ || '2.4.1.unknown';

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
    listen('tray-click', () => {
      devLog('[PAGE] EVENT: tray-click received');
      devLog('[PAGE] EVENT: calling invoke show_window');
      invoke('show_window');
    }).then(fn => {
      unlistenTray = fn;
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
      unlistenShutdown = fn;
      devLog('[PAGE] onMount: app-shutdown listener registered');
    });

    devLog('[PAGE] onMount: setting up navigate listener');
    listen<string>('navigate', (event) => {
      devLog('[PAGE] EVENT: navigate received:', event.payload);
      currentView.set(event.payload as View);
    }).then(fn => unlisten.push(fn));

    devLog('[PAGE] onMount: setting up open-logs-folder listener');
    listen('open-logs-folder', async () => {
      devLog('[PAGE] EVENT: open-logs-folder received');
      try {
        await invoke('open_logs_folder');
      } catch (e) {
        console.error('[PAGE] EVENT: open_logs_folder FAILED:', e);
      }
    }).then(fn => unlisten.push(fn));

    devLog('[PAGE] onMount: setting up show-about listener');
    listen('show-about', () => {
      devLog('[PAGE] EVENT: show-about received');
      currentView.set('about');
    }).then(fn => unlisten.push(fn));

    return () => {
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
    <span>Loading...</span>
  </div>
{:else}
  <div class="app-container">
    {#if $currentView === 'onboarding'}
      <Onboarding />
    {:else if $currentView === 'dashboard'}
      <Dashboard />
    {:else if $currentView === 'settings'}
      <Settings />
    {:else if $currentView === 'logs'}
      <LogViewer />
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
    background: var(--bg-primary);
    color: var(--text-secondary);
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
    color: var(--text-secondary);
    opacity: 0.6;
    pointer-events: none;
  }
</style>
