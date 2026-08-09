<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, loadConfig, type AppConfig } from '$lib/stores/config';
  import { authFlow, setSpotifyPhase, setTeamsPhase } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';
  import PageHeader from './PageHeader.svelte';

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
    // Teams re-auth is NOT auto-refreshing in general (device-code
    // refresh failures land the user in a re-auth flow — see #151,
    // #157), so surface the Teams reconnect path honestly.
    needsTeams = true;

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
  <PageHeader title="Reconnect" onBack={goToDashboard} showThemeToggle={false} />

  <div class="content">
    <p class="description">
      Your session expired. Reconnect below to resume syncing.
    </p>

    <section class="card">
      <header class="section-header">
        <h2>Spotify</h2>
        <span class="badge"
          class:success={authFlow.spotify.phase === 'done'}
          class:warning={authFlow.spotify.phase === 'waiting'}
          class:error={!!authFlow.spotify.error || needsSpotify}>
          <span class="dot"></span>
          {#if authFlow.spotify.phase === 'done'}Connected
          {:else if needsSpotify}Missing credentials
          {:else if authFlow.spotify.phase === 'waiting'}Waiting…
          {:else if authFlow.spotify.error}Failed
          {:else}Ready to reconnect{/if}
        </span>
      </header>

      {#if authFlow.spotify.phase === 'done'}
        <p class="hint">Spotify reconnected successfully.</p>
      {:else if needsSpotify}
        <p class="hint">Spotify credentials are not configured on this machine.</p>
      {:else if authFlow.spotify.phase === 'waiting'}
        <p class="hint">Complete authentication in the opened browser window.</p>
      {:else if authFlow.spotify.error}
        <p class="error-message" role="alert">{authFlow.spotify.error}</p>
        <button class="btn-full" onclick={reconnectSpotify}>Try again</button>
      {:else}
        <p class="hint">Click below to reconnect your Spotify account.</p>
        <button class="btn-full" onclick={reconnectSpotify}>Reconnect Spotify</button>
      {/if}
    </section>

    {#if needsSpotify}
      <div class="info-box card">
        <div class="info-icon">⚠</div>
        <div>
          <strong>Missing Spotify credentials?</strong>
          <p class="hint">You'll need to re-enter your Client ID and Client Secret.</p>
        </div>
        <button class="btn-secondary" onclick={goToOnboarding}>Go to full setup</button>
      </div>
    {/if}

    {#if authFlow.spotify.phase === 'done'}
      <button class="btn-full" onclick={goToDashboard}>Back to dashboard</button>
    {/if}
  </div>
</div>

<style>
  .reconnect {
    padding: var(--sp-5);
    max-width: 640px;
    margin: 0 auto;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }


  .content {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .description { color: var(--fg-muted); font-size: var(--fs-base); }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-3);
  }
  .section-header h2 {
    font-size: var(--fs-md);
    font-weight: 600;
  }


  .info-box {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: var(--sp-3);
    align-items: center;
    background: var(--bg-elevated);
  }
  .info-icon {
    width: 36px; height: 36px;
    border-radius: var(--r-md);
    background: var(--warning-soft);
    color: var(--warning);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: var(--fs-lg);
  }
  .info-box strong { color: var(--fg); font-size: var(--fs-base); display: block; margin-bottom: 2px; }
  .info-box .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  .hint { font-size: var(--fs-sm); color: var(--fg-subtle); }
  .error-message {
    color: var(--danger);
    background: var(--danger-soft);
    padding: var(--sp-3);
    border-radius: var(--r-md);
    font-size: var(--fs-sm);
  }

  .btn-full {
    width: 100%;
    padding: var(--sp-3) var(--sp-5);
    font-size: var(--fs-md);
  }
</style>
