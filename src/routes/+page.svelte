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

  let ready = $state(false);

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
      devLog('[PAGE] onDestroy: EXIT');
    };
  });

  devLog('[PAGE] currentView value:', $currentView);
</script>

{#if !ready}
  <div class="loading">
    <span>Loading...</span>
  </div>
{:else if $currentView === 'onboarding'}
  <Onboarding />
{:else if $currentView === 'dashboard'}
  <Dashboard />
{:else if $currentView === 'settings'}
  <Settings />
{:else if $currentView === 'logs'}
  <LogViewer />
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
</style>
