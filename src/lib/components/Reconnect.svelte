<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, loadConfig, type AppConfig } from '$lib/stores/config';
  import { authFlow, setSpotifyPhase, setTeamsPhase } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';

  let needsSpotify = $state(false);
  let needsTeams = $state(false);

  let unlisten: (() => void) | null = null;

  onMount(async () => {
    devLog('[RECONNECT] onMount: ENTRY');
    await loadConfig();

    // The client_secret no longer lives in the config — it lives in the OS
    // keychain. We check both client_id (in config) and the keychain
    // presence. See issue #9.
    const hasClientId = !!$configStore.spotify.client_id
      && $configStore.spotify.client_id.trim() !== '';
    const hasClientSecret = await invoke<boolean>('is_spotify_client_secret_set');
    needsSpotify = !hasClientId || !hasClientSecret;
    needsTeams = false; // Teams uses device code flow which typically auto-refreshes

    devLog('[RECONNECT] needsSpotify=', needsSpotify, 'needsTeams=', needsTeams);

    unlisten = await useAuthListeners({
      onSpotifyComplete: () => {
        devLog('[RECONNECT] EVENT: spotify-auth-complete received');
        setSpotifyPhase('done');
      },
      onSpotifyFailed: (payload) => {
        devLog('[RECONNECT] EVENT: spotify-auth-failed:', payload);
        setSpotifyPhase('error', String(payload));
      },
      onTeamsComplete: () => {
        devLog('[RECONNECT] EVENT: teams-auth-complete received');
        setTeamsPhase('done');
      },
      onTeamsFailed: (payload) => {
        devLog('[RECONNECT] EVENT: teams-auth-failed:', payload);
        setTeamsPhase('error', String(payload));
      }
    });

    // Auto-start Spotify reconnect only if credentials exist
    if (!needsSpotify && authFlow.spotify.phase !== 'done' && authFlow.spotify.phase !== 'waiting') {
      await reconnectSpotify();
    }
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function reconnectSpotify() {
    if (authFlow.spotify.phase === 'waiting' || authFlow.spotify.phase === 'done' || needsSpotify) return;
    devLog('[RECONNECT] reconnectSpotify: ENTRY');
    // Re-check the keychain: the user may have wiped it since the page
    // loaded. If the secret is gone we cannot complete the auth flow
    // without re-onboarding, so bail. See issue #9.
    const hasSecret = await invoke<boolean>('is_spotify_client_secret_set');
    if (!hasSecret) {
      devLog('[RECONNECT] reconnectSpotify: keychain empty, redirecting to onboarding');
      needsSpotify = true;
      return;
    }
    setSpotifyPhase('waiting');
    try {
      // Use the dedicated reconnect IPC — reads client_secret from the
      // OS keychain (set during Onboarding) instead of overwriting it
      // with an empty string. See issues #9, #67.
      await invoke('start_spotify_reconnect', {
        clientId: $configStore.spotify.client_id,
        redirectUri: 'presencejam://callback'
      });
    } catch (e) {
      devLog('[RECONNECT] reconnectSpotify: invoke failed:', e);
      setSpotifyPhase('error', String(e));
    }
  }

  function goToDashboard() {
    currentView.set('dashboard');
  }

  function goToOnboarding() {
    currentView.set('onboarding');
  }
</script>

<div class="reconnect">
  <header class="header">
    <button class="back-btn" onclick={goToDashboard}>← Back</button>
    <h1>Reconnect</h1>
  </header>

  <div class="content">
    <p class="description">
      Your session has expired. Reconnect below to resume syncing.
    </p>

    <section class="card">
      <h2>Spotify</h2>
      {#if authFlow.spotify.phase === 'done'}
        <div class="status success">
          <span class="badge success">Connected</span>
          <span>Spotify reconnected successfully</span>
        </div>
      {:else if needsSpotify}
        <div class="status error">
          <span class="badge error">Missing Credentials</span>
          <span>Spotify credentials are not configured</span>
        </div>
      {:else if authFlow.spotify.phase === 'waiting'}
        <div class="status waiting">
          <span class="badge warning">Waiting...</span>
          <span>Complete authentication in the opened browser</span>
        </div>
        <p class="hint">If the browser didn't open automatically, check your default browser and complete the Spotify authorization.</p>
      {:else if authFlow.spotify.error}
        <div class="status error">
          <span class="badge error">Failed</span>
          <span>{authFlow.spotify.error}</span>
        </div>
        <button class="btn-primary" onclick={reconnectSpotify}>Try Again</button>
      {:else}
        <p class="hint">Click below to reconnect your Spotify account.</p>
        <button class="btn-primary" onclick={reconnectSpotify}>Reconnect Spotify</button>
      {/if}
    </section>

    {#if authFlow.spotify.phase === 'done'}
      <div class="actions">
        <button class="btn-full" onclick={goToDashboard}>
          Back to Dashboard
        </button>
      </div>
    {/if}

    {#if needsSpotify}
      <div class="info-box">
        <p>Missing Spotify credentials? You'll need to enter your Client ID and Secret.</p>
        <button class="btn-secondary" onclick={goToOnboarding}>Go to Full Setup</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .reconnect {
    padding: 20px;
    max-width: 600px;
    margin: 0 auto;
    height: 100vh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 24px;
  }

  .back-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 6px 12px;
    font-size: 13px;
  }

  .back-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  h1 {
    font-size: 24px;
    font-weight: 600;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .description {
    color: var(--text-secondary);
    font-size: 14px;
  }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 20px;
  }

  h2 {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 12px;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 14px;
  }

  .badge {
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
  }

  .badge.success {
    background: rgba(74, 222, 128, 0.15);
    color: var(--color-success);
  }

  .badge.warning {
    background: rgba(251, 191, 36, 0.15);
    color: #fbbf24;
  }

  .badge.error {
    background: rgba(239, 68, 68, 0.15);
    color: var(--color-error);
  }

  .hint {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 8px;
  }

  .info-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .info-box p {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .actions {
    margin-top: 8px;
  }

  button {
    width: 100%;
    padding: 12px 16px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-primary {
    background: var(--color-accent);
    border: none;
    color: white;
    margin-top: 12px;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-secondary {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background: var(--bg-elevated);
  }

  .btn-full {
    background: var(--color-accent);
    border: none;
    color: white;
  }
</style>
