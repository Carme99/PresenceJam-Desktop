<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { currentView, type View } from '$lib/stores/app';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Dashboard from '$lib/components/Dashboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';

  let ready = $state(false);

  onMount(() => {
    let unlistenTray: (() => void) | undefined;

    (async () => {
      try {
        const complete = await invoke<boolean>('is_onboarding_complete');
        currentView.set(complete ? 'dashboard' : 'onboarding');
      } catch {
        currentView.set('onboarding');
      }
      ready = true;
    })();

    listen('tray-click', () => {
      invoke('show_window');
    }).then(fn => {
      unlistenTray = fn;
    });

    return () => {
      if (unlistenTray) unlistenTray();
    };
  });
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
