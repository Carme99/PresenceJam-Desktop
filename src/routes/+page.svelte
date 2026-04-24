<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { currentView, type View } from '$lib/stores/app';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Dashboard from '$lib/components/Dashboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import About from '$lib/components/About.svelte';

  // Build info — injected at build time via vite.config.js define
  const BUILD = import.meta.env.__APP_BUILD__ || '2.3.6.unknown';

  let ready = $state(false);
  let unlisten: (() => void)[] = [];

  onMount(() => {
    console.log('[PAGE] onMount: ENTRY');

    (async () => {
      console.log('[PAGE] onMount: calling invoke is_onboarding_complete');
      try {
        const complete = await invoke<boolean>('is_onboarding_complete');
        console.log('[PAGE] onMount: is_onboarding_complete SUCCESS, complete=', complete);
        currentView.set(complete ? 'dashboard' : 'onboarding');
        console.log('[PAGE] onMount: currentView set to:', complete ? 'dashboard' : 'onboarding');
      } catch (e) {
        console.error('[PAGE] onMount: is_onboarding_complete FAILED:', e);
        currentView.set('onboarding');
        console.log('[PAGE] onMount: currentView set to onboarding (from error)');
      }
      ready = true;
      console.log('[PAGE] onMount: ready=true');
    })();

    console.log('[PAGE] onMount: setting up tray-click listener');
    listen('tray-click', () => {
      console.log('[PAGE] EVENT: tray-click received');
      invoke('show_window');
    }).then(fn => unlisten.push(fn));

    console.log('[PAGE] onMount: setting up app-shutdown listener');
    listen('app-shutdown', async () => {
      console.log('[PAGE] EVENT: app-shutdown received');
      try {
        await invoke('app_exit');
        console.log('[PAGE] EVENT: app_exit SUCCESS');
      } catch (e) {
        console.error('[PAGE] EVENT: app_exit FAILED:', e);
      }
    }).then(fn => unlisten.push(fn));

    console.log('[PAGE] onMount: setting up navigate listener');
    listen<string>('navigate', (event) => {
      console.log('[PAGE] EVENT: navigate received:', event.payload);
      currentView.set(event.payload as View);
    }).then(fn => unlisten.push(fn));

    console.log('[PAGE] onMount: setting up open-logs-folder listener');
    listen('open-logs-folder', async () => {
      console.log('[PAGE] EVENT: open-logs-folder received');
      try {
        await invoke('open_logs_folder');
      } catch (e) {
        console.error('[PAGE] EVENT: open_logs_folder FAILED:', e);
      }
    }).then(fn => unlisten.push(fn));

    console.log('[PAGE] onMount: setting up show-about listener');
    listen('show-about', () => {
      console.log('[PAGE] EVENT: show-about received');
      currentView.set('about');
    }).then(fn => unlisten.push(fn));

    return () => {
      console.log('[PAGE] onDestroy: ENTRY');
      unlisten.forEach(fn => fn());
      console.log('[PAGE] onDestroy: EXIT');
    };
  });

  console.log('[PAGE] currentView value:', $currentView);
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
