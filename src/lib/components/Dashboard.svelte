<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import type { TrackInfo } from '$lib/stores/spotify';
  import { devLog } from '$lib/utils/dev';

  let isSyncing = $state(false);
  let isToggling = $state(false);
  let spotifyConnected = $state(false);
  let teamsConnected = $state(false);
  let currentTrack = $state<TrackInfo | null>(null);
  let statusPreview = $state('Not configured');
  let displayError = $state('');
  let displayErrorTimeout: ReturnType<typeof setTimeout> | null = null;
  let unlisten: (() => void)[] = [];

  onDestroy(() => {
    unlisten.forEach(fn => fn());
    if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
  });

  onMount(async () => {
    devLog('[DASHBOARD] onMount: ENTRY');
    
    try {
      devLog('[DASHBOARD] onMount: calling invoke get_sync_status');
      const status = await invoke<any>('get_sync_status');
      console.info('[DASHBOARD] initial sync status:', {
        is_syncing: status.is_syncing,
        spotify_connected: status.spotify_connected,
        teams_connected: status.teams_connected,
        current_track: status.current_track?.title ?? null
      });
      
      isSyncing = status.is_syncing;
      spotifyConnected = status.spotify_connected;
      teamsConnected = status.teams_connected;
      currentTrack = status.current_track;
      await updateMenuState();
    } catch (e) {
      console.error('[DASHBOARD] onMount: get_sync_status FAILED:', e);
    }

    devLog('[DASHBOARD] onMount: setting up spotify-track-changed listener');
    unlisten.push(await listen('spotify-track-changed', async (event: any) => {
      devLog('[DASHBOARD] EVENT: spotify-track-changed received');
      devLog('[DASHBOARD] EVENT: track.title=', event.payload.title);
      devLog('[DASHBOARD] EVENT: track.artist=', event.payload.artist);
      currentTrack = event.payload;
      await updateMenuState();
    }));
    
    devLog('[DASHBOARD] onMount: setting up presence-updated listener');
    unlisten.push(await listen('presence-updated', (event: any) => {
      devLog('[DASHBOARD] EVENT: presence-updated received');
      devLog('[DASHBOARD] EVENT: status=', event.payload.status);
      statusPreview = event.payload.status;
    }));
    
    devLog('[DASHBOARD] onMount: setting up presence-cleared listener');
    unlisten.push(await listen('presence-cleared', async () => {
      devLog('[DASHBOARD] EVENT: presence-cleared received');
      currentTrack = null;
      statusPreview = 'No track playing';
      devLog('[DASHBOARD] EVENT: currentTrack=null, statusPreview="No track playing"');
      await updateMenuState();
    }));
    
    devLog('[DASHBOARD] onMount: setting up error listener');
    unlisten.push(await listen('error', (event: any) => {
      console.error('[DASHBOARD] EVENT: error received:', event.payload);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = String(event.payload);
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    }));
    
    devLog('[DASHBOARD] onMount: setting up toggle-pause listener');
    unlisten.push(await listen('toggle-pause', async () => {
      if (isToggling) return;
      devLog('[DASHBOARD] EVENT: toggle-pause received');
      devLog('[DASHBOARD] EVENT: isSyncing=', isSyncing);

      isToggling = true;
      try {
        if (isSyncing) {
          devLog('[DASHBOARD] EVENT: calling invoke stop_syncing');
          await invoke('stop_syncing');
          isSyncing = false;
          devLog('[DASHBOARD] EVENT: isSyncing=false');
        } else {
          devLog('[DASHBOARD] EVENT: calling invoke start_syncing');
          await invoke('start_syncing');
          isSyncing = true;
          devLog('[DASHBOARD] EVENT: isSyncing=true');
        }
        await updateMenuState();
      } finally {
        isToggling = false;
      }
    }));
    devLog('[DASHBOARD] onMount: setting up sync-started listener');
    unlisten.push(await listen('sync-started', () => {
      devLog('[DASHBOARD] EVENT: sync-started received');
      isSyncing = true;
      devLog('[DASHBOARD] EVENT: isSyncing=true');
      updateMenuState();
    }));

    devLog('[DASHBOARD] onMount: setting up sync-stopped listener');
    unlisten.push(await listen('sync-stopped', () => {
      devLog('[DASHBOARD] EVENT: sync-stopped received');
      isSyncing = false;
      devLog('[DASHBOARD] EVENT: isSyncing=false');
      updateMenuState();
    }));
  });

  async function toggleSync() {
    if (isToggling) return;
    devLog('[DASHBOARD] toggleSync: ENTRY');
    devLog('[DASHBOARD] toggleSync: isSyncing=', isSyncing);

    isToggling = true;
    try {
      if (isSyncing) {
        devLog('[DASHBOARD] toggleSync: calling invoke stop_syncing');
        await invoke('stop_syncing');
        isSyncing = false;
        devLog('[DASHBOARD] toggleSync: isSyncing=false');
      } else {
        devLog('[DASHBOARD] toggleSync: calling invoke start_syncing');
        await invoke('start_syncing');
        isSyncing = true;
        devLog('[DASHBOARD] toggleSync: isSyncing=true');
      }
      await updateMenuState();
    } finally {
      isToggling = false;
    }

    devLog('[DASHBOARD] toggleSync: EXIT');
  }

  function openSettings() {
    devLog('[DASHBOARD] openSettings: ENTRY');
    currentView.set('settings');
    devLog('[DASHBOARD] openSettings: EXIT');
  }

  function openLogs() {
    devLog('[DASHBOARD] openLogs: ENTRY');
    currentView.set('logs');
    devLog('[DASHBOARD] openLogs: EXIT');
  }

  function formatDuration(ms: number): string {
    const secs = Math.floor(ms / 1000);
    const mins = Math.floor(secs / 60);
    const remainingSecs = secs % 60;
    return `${mins}:${remainingSecs.toString().padStart(2, '0')}`;
  }

  let progressPercent = $derived(
    currentTrack && currentTrack.duration_ms > 0
      ? (currentTrack.progress_ms / currentTrack.duration_ms) * 100
      : 0
  );

  // Helper to update tray menu state
  async function updateMenuState() {
    try {
      await invoke('update_tray_menu_state', {
        isSyncing: isSyncing,
        currentTrack: currentTrack
      });
    } catch (e) {
      console.error('[DASHBOARD] updateMenuState failed:', e);
    }
  }
</script>

<div class="dashboard">
  <header>
    <div class="header-left">
      <h1>PresenceJam</h1>
      <div class="badges">
        <span class="badge" class:success={spotifyConnected} class:error={!spotifyConnected}>
          {spotifyConnected ? '✓' : '✗'} Spotify
        </span>
        <span class="badge" class:success={teamsConnected} class:error={!teamsConnected}>
          {teamsConnected ? '✓' : '✗'} Teams
        </span>
      </div>
    </div>
    <div class="header-right">
      <button class="icon-btn" onclick={openLogs} title="Logs">📋</button>
      <button class="icon-btn" onclick={openSettings} title="Settings">⚙️</button>
      <button class="icon-btn sync-btn" onclick={toggleSync} disabled={isToggling} title={isSyncing ? 'Pause' : 'Resume'}>
        {isSyncing ? '⏸' : '▶'}
      </button>
    </div>
  </header>

  {#if displayError}
    <div class="error-banner">{displayError}</div>
  {/if}

  <main>
    {#if !spotifyConnected || !teamsConnected}
      <div class="setup-card card">
        <h2>Setup Required</h2>
        <p>Complete onboarding to connect Spotify and Teams.</p>
        <button onclick={() => { devLog('[DASHBOARD] Go to Setup clicked'); currentView.set('onboarding'); }}>Go to Setup</button>
      </div>
    {:else if currentTrack}
      <div class="track-card card">
        {#if currentTrack.album_art_url}
          <img src={currentTrack.album_art_url} alt="Album art" class="album-art" />
        {:else}
          <div class="album-art placeholder">🎵</div>
        {/if}
        <div class="track-info">
          <div class="track-title">{currentTrack.title}</div>
          <div class="track-artist">{currentTrack.artist}</div>
          <div class="track-album">{currentTrack.album}</div>
          
          {#if currentTrack.is_playing}
            <div class="playing-indicator">
              <span class="pulse-dot"></span> Playing
            </div>
          {:else}
            <div class="paused-indicator">⏸ Paused</div>
          {/if}
          
          <div class="progress-bar">
            <div class="progress-fill" style="width: {progressPercent}%"></div>
          </div>
          <div class="progress-time">
            {formatDuration(currentTrack.progress_ms)} / {formatDuration(currentTrack.duration_ms)}
          </div>
        </div>
      </div>

      <div class="status-preview card">
        <h3>Your Teams Status</h3>
        <p class="status-text">{statusPreview}</p>
      </div>
    {:else}
      <div class="not-playing card">
        <div class="not-playing-icon">🎵</div>
        <h3>Not Playing</h3>
        <p>Start playing something on Spotify</p>
      </div>
    {/if}
  </main>
</div>

<style>
  .dashboard {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }
  .header-left { display: flex; align-items: center; gap: 16px; }
  h1 { font-size: 18px; font-weight: 600; }
  .badges { display: flex; gap: 8px; }
  .badge { padding: 4px 10px; border-radius: 999px; font-size: 12px; font-weight: 500; }
  .badge.success { background: rgba(74,222,128,0.15); color: var(--color-success); }
  .badge.error { background: rgba(239,68,68,0.15); color: var(--color-error); }
  .header-right { display: flex; gap: 8px; }
  .icon-btn { background: transparent; border: 1px solid var(--border-color); border-radius: 8px; padding: 8px; font-size: 16px; cursor: pointer; }
  .icon-btn:hover { background: var(--bg-elevated); }
  .sync-btn { color: var(--color-accent); }
  .error-banner { background: rgba(239,68,68,0.15); color: var(--color-error); padding: 12px 20px; text-align: center; font-size: 14px; border-bottom: 1px solid var(--color-error); }
  main { flex: 1; overflow-y: auto; padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  .card { background: var(--bg-surface); border: 1px solid var(--border-color); border-radius: 12px; padding: 20px; }
  .track-card { display: flex; gap: 16px; align-items: flex-start; }
  .album-art { width: 80px; height: 80px; border-radius: 8px; object-fit: cover; }
  .album-art.placeholder { background: var(--bg-elevated); display: flex; align-items: center; justify-content: center; font-size: 32px; }
  .track-info { flex: 1; }
  .track-title { font-size: 18px; font-weight: 600; margin-bottom: 4px; }
  .track-artist { color: var(--text-secondary); font-size: 14px; }
  .track-album { color: var(--text-secondary); font-size: 12px; margin-bottom: 8px; }
  .playing-indicator { display: flex; align-items: center; gap: 6px; color: var(--color-success); font-size: 12px; font-weight: 500; margin-bottom: 8px; }
  .paused-indicator { color: var(--text-secondary); font-size: 12px; font-weight: 500; margin-bottom: 8px; }
  .pulse-dot { width: 8px; height: 8px; background: var(--color-success); border-radius: 50%; animation: pulse 1.5s infinite; }
  @keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }
  .progress-bar { height: 4px; background: var(--bg-elevated); border-radius: 2px; overflow: hidden; margin-bottom: 4px; }
  .progress-fill { height: 100%; background: var(--color-accent); transition: width 1s; }
  .progress-time { font-size: 11px; color: var(--text-secondary); }
  .status-preview h3 { font-size: 12px; text-transform: uppercase; letter-spacing: 1px; color: var(--text-secondary); margin-bottom: 8px; }
  .status-text { font-size: 16px; color: var(--text-primary); }
  .not-playing { text-align: center; padding: 40px; }
  .not-playing-icon { font-size: 48px; margin-bottom: 16px; }
  .not-playing h3 { margin-bottom: 8px; }
  .not-playing p { color: var(--text-secondary); }
  .setup-card { text-align: center; padding: 40px; }
  .setup-card h2 { margin-bottom: 8px; }
  .setup-card p { color: var(--text-secondary); margin-bottom: 16px; }
</style>
