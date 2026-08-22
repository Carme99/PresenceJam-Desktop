<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  // Side-effect import — installs the module-level subscribe that
  // applies the persisted theme (and keeps it in sync with future
  // changes). Without this, theme only applies when Settings mounts.
  import '$lib/stores/theme';
  import { devLog } from '$lib/utils/dev';
  import UpdatePrompt from '$lib/components/UpdatePrompt.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { currentView } from '$lib/stores/app';

  // C7: this layout is shared by every webview window (the SPA fallback
  // hydrates it for detached Logs/Settings windows too). Reconnect flows,
  // auth navigation, and update checks are owned by the main window —
  // registering them per-window would run device-code/OAuth flows twice
  // when both windows mount. Detached windows only inherit the theme
  // side-effect import above.
  const isMainWindow = getCurrentWindow().label === 'main';
  import { authFlow, setTeamsPhase, setTeamsDeviceCode, setSpotifyPhase } from '$lib/stores/authFlow.svelte';
  import type { DeviceCodeResponse, TeamsTokens, AppConfig } from '$lib/types';

  devLog(`[LAYOUT] PresenceJam build: ${import.meta.env.VITE_APP_BUILD ?? 'dev build'}`);

  let playbackError = $state('');
  let playbackErrorTimeout: ReturnType<typeof setTimeout> | null = null;

  function showPlaybackError(msg: string) {
    playbackError = msg;
    if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
    playbackErrorTimeout = setTimeout(() => {
      playbackError = '';
      playbackErrorTimeout = null;
    }, 6000);
  }

  // Always-mounted listeners: teams + spotify reconnect + playback-error.
  // Settings no longer owns spotify-reconnect-required (issue #220) to
  // avoid missed events when the user is on Dashboard.
  onMount(() => {
    if (!isMainWindow) return;
    let unlistenTeams: (() => void) | null = null;
    let unlistenSpotify: (() => void) | null = null;
    let unlistenPlayback: (() => void) | null = null;
    let destroyed = false;

    listen('teams-reconnect-required', async () => {
      devLog('[LAYOUT] teams-reconnect-required received');
      setTeamsPhase('waiting');
      currentView.set('settings');
      try {
        const response = await invoke<DeviceCodeResponse>('start_teams_auth_device_code');
        setTeamsDeviceCode({
          userCode: response.user_code,
          verificationUrl: response.verification_url,
          deviceCode: response.device_code,
          interval: response.interval
        });
        try {
          await invoke('open_external_url', { url: response.verification_url });
        } catch (e) {
          console.warn('[LAYOUT] open_external_url failed:', e);
        }
        void pollTeamsAuth();
      } catch (e) {
        console.error('[LAYOUT] teams-reconnect-required: start_teams_auth_device_code failed:', e);
        setTeamsPhase('error', String(e));
      }
    }).then((u) => {
      if (destroyed) u();
      else unlistenTeams = u;
    });

    listen<string>('spotify-reconnect-required', async () => {
      devLog('[LAYOUT] spotify-reconnect-required received');
      setSpotifyPhase('waiting');
      currentView.set('settings');
      try {
        const hasSecret = await invoke<boolean>('is_spotify_client_secret_set');
        if (!hasSecret) {
          console.warn('[LAYOUT] spotify-reconnect-required: keychain empty, redirecting to onboarding');
          currentView.set('onboarding');
          return;
        }
        // Client ID lives in config; fetch it to drive start_spotify_reconnect.
        const cfg = await invoke<AppConfig>('load_config');
        const clientId = cfg.spotify.client_id;
        if (!clientId) {
          console.warn('[LAYOUT] spotify-reconnect-required: client_id empty, redirecting to onboarding');
          currentView.set('onboarding');
          return;
        }
        await invoke('start_spotify_reconnect', {
          clientId,
          redirectUri: 'presencejam://callback'
        });
      } catch (e) {
        console.error('[LAYOUT] start_spotify_reconnect failed:', e);
        setSpotifyPhase('error', String(e));
      }
    }).then((u) => {
      if (destroyed) u();
      else unlistenSpotify = u;
    });

    listen<string>('playback-error', (event) => {
      const msg = typeof event.payload === 'string' ? event.payload : String(event.payload);
      console.warn('[LAYOUT] playback-error received:', msg);
      showPlaybackError(msg);
    }).then((u) => {
      if (destroyed) u();
      else unlistenPlayback = u;
    });

    // Polls the backend for device-code completion. The cadence is
    // Rust-side; `interval` comes from the DeviceCodeResponse stored in
    // the authFlow store so the server's requested polling rate is
    // honored — see issue #152.
    async function pollTeamsAuth() {
      if (!authFlow.teams.deviceCode) return;
      setTeamsPhase('waiting');
      try {
        const tokens = await invoke<TeamsTokens>('poll_teams_auth', {
          deviceCode: authFlow.teams.deviceCode,
          interval: authFlow.teams.interval
        });
        if (tokens) {
          setTeamsPhase('done');
        }
      } catch (e) {
        console.error('[LAYOUT] poll_teams_auth failed:', e);
        setTeamsPhase('error', String(e));
      }
    }

    return () => {
      destroyed = true;
      unlistenTeams?.();
      unlistenSpotify?.();
      unlistenPlayback?.();
      if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
    };
  });

  onDestroy(() => {
    if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
  });
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href="/icon.svg" />
  <link rel="alternate icon" type="image/png" href="/favicon.png" />
  <meta name="color-scheme" content="dark light" />
</svelte:head>

<a class="skip-link" href="#main-content">Skip to main content</a>
<slot />
{#if playbackError}
  <div class="playback-toast" role="alert">
    <span class="toast-msg">{playbackError}</span>
    <button class="toast-dismiss" onclick={() => { playbackError = ''; if (playbackErrorTimeout) { clearTimeout(playbackErrorTimeout); playbackErrorTimeout = null; } }} aria-label="Dismiss">×</button>
  </div>
{/if}
{#if isMainWindow}<UpdatePrompt />{/if}

<style>
  .playback-toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-surface, #1e1e1e);
    color: var(--fg, #eee);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(90vw, 480px);
    box-shadow: 0 4px 16px rgba(0,0,0,0.3);
    z-index: 9999;
  }
  .toast-msg { font-size: 14px; line-height: 1.4; }
  .toast-dismiss {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 18px;
    cursor: pointer;
    padding: 2px 6px;
    opacity: 0.7;
  }
  .toast-dismiss:hover { opacity: 1; }

</style>
