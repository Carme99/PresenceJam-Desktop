<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
  import { currentView } from '$lib/stores/app';
  import { configStore, loadConfig } from '$lib/stores/config';
  import type { ErrorEventPayload, SyncStatus, TrackInfo } from '$lib/types';
  import { devLog } from '$lib/utils/dev';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import Logo from './Logo.svelte';

  let isSyncing = $state(false);
  let isToggling = $state(false);
  let spotifyConnected = $state(false);
  let teamsConnected = $state(false);
  let currentTrack = $state<TrackInfo | null>(null);
  let statusPreview = $state('Not configured');
  let displayError = $state('');
  // P2 (issue #3.0-P2): true while the status write is suppressed by a
  // busy/meeting presence; cleared on the next `presence-updated`.
  let presenceGated = $state(false);
  // P1 (issue #3.0-P1): label from `presence-availability-updated`
  // ('Listening (Available)' / 'Availability cleared').
  let availabilityLabel = $state('');
  let displayErrorTimeout: ReturnType<typeof setTimeout> | null = null;
  let unlisten: (() => void)[] = [];
  // 3.1.0 notifications — opt-in via localStorage, default off
  let notificationsEnabled = $state(false);
  let lastNotifiedId = '';

  onDestroy(() => {
    unlisten.forEach(fn => fn());
    if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
  });

  onMount(async () => {
    devLog('[DASHBOARD] onMount: ENTRY');

    try {
      devLog('[DASHBOARD] onMount: calling invoke get_sync_status');
      const status = await invoke<SyncStatus>('get_sync_status');
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

    // 3.1.0: notification opt-in gate — default off, enabled via localStorage flag
    try { notificationsEnabled = localStorage.getItem('notificationsEnabled') === 'true'; } catch {}

    devLog('[DASHBOARD] onMount: setting up spotify-track-changed listener');
    unlisten.push(await listen('spotify-track-changed', async (event: any) => {
      devLog('[DASHBOARD] EVENT: spotify-track-changed received');
      devLog('[DASHBOARD] EVENT: track.title=', event.payload.title);
      devLog('[DASHBOARD] EVENT: track.artist=', event.payload.artist);
      currentTrack = event.payload;
      await updateMenuState();
      if (notificationsEnabled && event.payload?.title) {
        const id = `${event.payload.title}::${event.payload.artist}`;
        if (id !== lastNotifiedId) {
          lastNotifiedId = id;
          let granted = false;
          try { granted = await isPermissionGranted(); } catch {}
          if (!granted) { try { granted = (await requestPermission()) === 'granted'; } catch {} }
          if (granted) {
            const body = `${event.payload.artist} — ${event.payload.album ?? ''}`.trim();
            try { sendNotification({ title: event.payload.title, body, icon: event.payload.album_art_url || undefined }); } catch {}
          }
        }
      }
    }));

    devLog('[DASHBOARD] onMount: setting up presence-updated listener');
    unlisten.push(await listen('presence-updated', (event: any) => {
      devLog('[DASHBOARD] EVENT: presence-updated received');
      devLog('[DASHBOARD] EVENT: status=', event.payload.status);
      statusPreview = event.payload.status;
      // A real status write means the gate is no longer suppressing —
      // clear the chip (issue #3.0-P2).
      presenceGated = false;
    }));

    devLog('[DASHBOARD] onMount: setting up presence-cleared listener');
    unlisten.push(await listen('presence-cleared', async () => {
      devLog('[DASHBOARD] EVENT: presence-cleared received');
      currentTrack = null;
      statusPreview = 'No track playing';
      devLog('[DASHBOARD] EVENT: currentTrack=null, statusPreview="No track playing"');
      await updateMenuState();
    }));

    devLog('[DASHBOARD] onMount: setting up presence-gated listener');
    unlisten.push(await listen('presence-gated', (event: any) => {
      devLog('[DASHBOARD] EVENT: presence-gated received');
      devLog('[DASHBOARD] EVENT: reason=', event.payload?.reason);
      presenceGated = true;
    }));

    devLog('[DASHBOARD] onMount: setting up presence-availability-updated listener');
    unlisten.push(await listen('presence-availability-updated', (event: any) => {
      devLog('[DASHBOARD] EVENT: presence-availability-updated received');
      availabilityLabel = event.payload?.label ?? '';
    }));

    devLog('[DASHBOARD] onMount: setting up error listener');
    unlisten.push(await listen<ErrorEventPayload>('error', (event) => {
      const payload = event.payload;
      console.error('[DASHBOARD] EVENT: error received:', payload);
      // Issue #79: only `severity: "error"` (i.e. an error the polling
      // loop did not automatically recover from) pops the red banner.
      // `severity: "warning"` events (e.g. a 401 that triggered token
      // refresh, a 429 that triggered backoff) are logged to the
      // console for the developer but do not alarm-fatigue the user
      // with a banner that disappears during the next successful poll.
      if (payload.severity !== 'error') {
        return;
      }
      const message = typeof payload.message === 'string'
        ? payload.message
        : String(payload);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = message;
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

    devLog('[DASHBOARD] onMount: setting up polling-thread-panicked listener');
    unlisten.push(await listen('polling-thread-panicked', () => {
      // Rust side resets is_syncing in polling.rs:321, but the JS-side
      // mirror (this rune) was not being flipped — UI would stay
      // "Syncing" forever after a thread panic. See issue #33.
      devLog('[DASHBOARD] EVENT: polling-thread-panicked received');
      isSyncing = false;
      devLog('[DASHBOARD] EVENT: isSyncing=false (panic recovery)');
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = 'Sync stopped unexpectedly. Please restart PresenceJam.';
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
      updateMenuState();
    }));

    devLog('[DASHBOARD] onMount: setting up reconnect-required listener');
    unlisten.push(await listen('reconnect-required', () => {
      // Generic reconnect signal from polling.rs:633 (e.g. when the
      // auth refresh loop has been failing for too long). The
      // provider-specific events are handled elsewhere:
      // spotify-reconnect-required in +layout.svelte (issue #220),
      // teams-reconnect-required in +layout.svelte (issue #157);
      // this is the catch-all that takes the user to the reconnect view.
      devLog('[DASHBOARD] EVENT: reconnect-required received');
      isSyncing = false;
      devLog('[DASHBOARD] EVENT: isSyncing=false (reconnect)');
      currentView.set('reconnect');
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

  function openAbout() {
    currentView.set('about');
  }

  let goToSetupHint = $state('');
  let goToSetupDisabled = $state(false);

  async function goToSetup() {
    devLog('[DASHBOARD] goToSetup: ENTRY');
    try {
      await loadConfig();
      // The client_secret now lives in the OS keychain. We check both the
      // config (client_id) and the keychain (client_secret). See issue #9.
      const hasClientId = !!$configStore.spotify.client_id
        && $configStore.spotify.client_id.trim() !== '';
      const hasClientSecret = await invoke<boolean>('is_spotify_client_secret_set');
      const hasSpotifyCredentials = hasClientId && hasClientSecret;
      devLog('[DASHBOARD] goToSetup: hasSpotifyCredentials=', hasSpotifyCredentials);

      if (hasSpotifyCredentials) {
        // Credentials exist, go to simplified reconnect flow
        devLog('[DASHBOARD] goToSetup: navigating to reconnect');
        currentView.set('reconnect');
      } else {
        // Missing credentials, need full onboarding
        devLog('[DASHBOARD] goToSetup: navigating to onboarding');
        currentView.set('onboarding');
      }
    } catch (e) {
      console.warn('[DASHBOARD] goToSetup failed:', e);
      goToSetupHint = 'Unable to check credentials — please try again.';
      goToSetupDisabled = true;
      setTimeout(() => { goToSetupHint = ''; goToSetupDisabled = false; }, 4000);
    }
    devLog('[DASHBOARD] goToSetup: EXIT');
  }

  function formatDuration(ms: number): string {
    const secs = Math.floor(ms / 1000);
    const mins = Math.floor(secs / 60);
    const remainingSecs = secs % 60;
    return `${mins}:${remainingSecs.toString().padStart(2, '0')}`;
  }

  // `progress_ms` is null for live/unknown-position streams (Spotify
  // documents it as nullable — see issue #165); treat null as "no known
  // position" rather than position 0.
  let progressPercent = $derived(
    currentTrack && currentTrack.progress_ms != null && currentTrack.duration_ms > 0
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
      <Logo size={32} />
      <div class="title">
        <h1>PresenceJam</h1>
        <div class="badges">
          <span class="badge" class:success={spotifyConnected} class:error={!spotifyConnected}>
            <span class="dot"></span>{spotifyConnected ? 'Spotify' : 'Spotify off'}
          </span>
          <span class="badge" class:success={teamsConnected} class:error={!teamsConnected}>
            <span class="dot"></span>{teamsConnected ? 'Teams' : 'Teams off'}
          </span>
          {#if isSyncing}
            <span class="badge accent"><span class="dot pulse"></span>Syncing</span>
          {/if}
        </div>
      </div>
    </div>
    <div class="header-right">
      <button class="icon-btn" onclick={toggleTheme} title="Toggle theme" aria-label="Toggle theme">
        {$theme === 'dark' ? '☀' : '☾'}
      </button>
      <button class="icon-btn" onclick={openLogs} title="Logs" aria-label="Open logs">📋</button>
      <button class="icon-btn" onclick={openSettings} title="Settings" aria-label="Open settings">⚙</button>
      <button class="icon-btn" onclick={openAbout} title="About" aria-label="About PresenceJam">ⓘ</button>
      <button class="icon-btn primary" class:is-on={isSyncing} onclick={toggleSync}
        disabled={isToggling} aria-label={isSyncing ? 'Pause sync' : 'Resume sync'}
        title={isSyncing ? 'Pause sync' : 'Resume sync'}>
        {isSyncing ? '⏸' : '▶'}
      </button>
    </div>
  </header>

  {#if displayError}
    <div class="error-banner" role="alert">{displayError}</div>
  {/if}

  <main>
    {#if presenceGated}
      <div class="presence-chip" role="status">Status paused while you're busy/in a meeting</div>
    {/if}
    {#if availabilityLabel}
      <div class="availability-chip" role="status">{availabilityLabel}</div>
    {/if}
    {#if !spotifyConnected || !teamsConnected}
      <div class="setup-card card">
        <div class="setup-icon"><Logo size={56} /></div>
        <h2>Setup required</h2>
        <p>Connect Spotify and Microsoft Teams so your now-playing tracks can drive your Teams status.</p>
        <div class="setup-actions">
          <button class="btn-full" onclick={goToSetup} disabled={goToSetupDisabled}>Continue setup</button>
          {#if goToSetupHint}
            <p class="hint" role="status">{goToSetupHint}</p>
          {/if}
        </div>
      </div>
    {:else if currentTrack}
      <div class="track-card card">
        {#if currentTrack.album_art_url}
          <img src={currentTrack.album_art_url} alt="" class="album-art" />
        {:else}
          <div class="album-art placeholder" aria-hidden="true">🎵</div>
        {/if}
        <div class="track-info">
          <div class="track-title">{currentTrack.title}</div>
          <div class="track-artist">{currentTrack.artist}</div>
          <div class="track-album">{currentTrack.album}</div>

          {#if currentTrack.is_playing}
            <div class="playing-indicator">
              <span class="pulse-dot" aria-hidden="true"></span>
              <span>Playing</span>
            </div>
          {:else}
            <div class="paused-indicator"><span aria-hidden="true">⏸</span> Paused</div>
          {/if}

          <div class="progress-bar" aria-hidden="true">
            <div class="progress-fill" style="width: {progressPercent}%"></div>
          </div>
          <div class="progress-time">
            {#if currentTrack.progress_ms != null}
              {formatDuration(currentTrack.progress_ms)} / {formatDuration(currentTrack.duration_ms)}
            {:else}
              <span class="live-label" aria-label="Live stream — position unknown">LIVE</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="status-preview card">
        <h3>Your Teams status</h3>
        <p class="status-text" aria-live="polite">{statusPreview}</p>
      </div>
    {:else}
      <div class="not-playing card">
        <div class="not-playing-icon" aria-hidden="true">
          <Logo size={64} />
        </div>
        <h3>Nothing playing</h3>
        <p>Start something on Spotify and we'll pipe it through to Teams.</p>
      </div>
    {/if}
  </main>
</div>

<style>
  .dashboard {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-4);
    padding: var(--sp-4) var(--sp-5);
    border-bottom: 1px solid var(--border);
    background: var(--bg-base);
  }
  .header-left {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-width: 0;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  h1 {
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .header-right {
    display: flex;
    gap: var(--sp-2);
    flex-shrink: 0;
  }
  .icon-btn {
    width: 36px;
    height: 36px;
  }
  .icon-btn.primary {
    color: var(--accent-text);
    border-color: var(--border);
  }
  .icon-btn.primary.is-on {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }

  .error-banner {
    background: var(--danger-soft);
    color: var(--danger);
    padding: var(--sp-3) var(--sp-5);
    text-align: center;
    font-size: var(--fs-sm);
    font-weight: 600;
    border-bottom: 1px solid var(--danger);
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    padding: var(--sp-5);
  }

  /* Presence indicators (issue #3.0-P1/P2): the gate chip while the status
     write is suppressed, and the availability-sync bubble state. */
  .presence-chip,
  .availability-chip {
    align-self: flex-start;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-2) var(--sp-3);
    font-size: var(--fs-sm);
    color: var(--fg);
  }

  .setup-card {
    text-align: center;
    padding: var(--sp-9) var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
  }
  .setup-icon {
    width: 72px;
    height: 72px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    border-radius: var(--r-lg);
    margin-bottom: var(--sp-2);
  }
  .setup-card h2 { font-size: var(--fs-2xl); }
  .setup-card p {
    color: var(--fg-muted);
    max-width: 36ch;
  }
  .setup-actions {
    width: 100%;
    max-width: 280px;
    margin-top: var(--sp-3);
  }

  .track-card {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: var(--sp-4);
    align-items: flex-start;
  }
  .album-art {
    width: 88px;
    height: 88px;
    border-radius: var(--r-md);
    object-fit: cover;
    background: var(--bg-elevated);
  }
  .album-art.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 32px;
  }
  .track-info {
    min-width: 0;
  }
  .track-title {
    font-size: var(--fs-lg);
    font-weight: 600;
    margin-bottom: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-artist {
    color: var(--fg);
    font-size: var(--fs-sm);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-album {
    color: var(--fg-subtle);
    font-size: var(--fs-xs);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .playing-indicator {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--success);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: var(--sp-3) 0 var(--sp-2);
  }
  .paused-indicator {
    color: var(--fg-subtle);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: var(--sp-3) 0 var(--sp-2);
  }
  .pulse-dot {
    width: 8px; height: 8px;
    background: var(--success);
    border-radius: 50%;
    box-shadow: 0 0 0 0 var(--success);
    animation: pulse 1.5s var(--ease-out) infinite;
  }
  @keyframes pulse {
    0%   { box-shadow: 0 0 0 0 var(--success-soft); }
    70%  { box-shadow: 0 0 0 8px transparent; }
    100% { box-shadow: 0 0 0 0 transparent; }
  }
  .progress-bar {
    height: 4px;
    background: var(--bg-elevated);
    border-radius: var(--r-pill);
    overflow: hidden;
    margin: var(--sp-3) 0 var(--sp-1);
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent), var(--accent-hover));
    border-radius: var(--r-pill);
    transition: width var(--dur-slow) linear;
  }
  .progress-time {
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
    font-variant-numeric: tabular-nums;
  }
  .live-label {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--success);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .status-preview {
    padding: var(--sp-4) var(--sp-5);
  }
  .status-preview h3 {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--fg-subtle);
    margin-bottom: var(--sp-2);
    font-weight: 600;
  }
  .status-text {
    font-size: var(--fs-md);
    color: var(--fg);
    word-break: break-word;
  }

  .not-playing {
    text-align: center;
    padding: var(--sp-9) var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
  }
  .not-playing-icon {
    width: 88px; height: 88px;
    background: var(--bg-elevated);
    border-radius: var(--r-lg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--sp-2);
  }
  .not-playing h3 { font-size: var(--fs-xl); }
</style>
