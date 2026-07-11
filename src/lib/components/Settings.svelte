<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, saveConfig, loadConfig, type AppConfig } from '$lib/stores/config';
  import type { SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import PageHeader from './PageHeader.svelte';
  import { theme } from '$lib/stores/theme';

  let localConfig = $state<AppConfig>({ ...$configStore });
  let isConnected = $state(false);
  let teamsStatusConnected = $state(false);
  let isSaving = $state(false);
  let saveMessage = $state('');
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;
  let spotifyAuthWaiting = $derived(authFlow.spotify.phase === 'waiting');
  let teamsAuthWaiting = $derived(authFlow.teams.phase === 'waiting');

  let previewText = $state('');

  // Live preview of the status format template. We delegate the
  // placeholder substitution to Rust (`preview_status`) so the Svelte
  // preview and the runtime polling loop share one implementation —
  // see issue #74. `$effect` updates `previewText` whenever the user
  // edits the format string. Using `$effect` over an `await` inside
  // `$derived` avoids a per-keystroke loading flash and keeps the
  // template a plain `{previewText}` interpolation.
  $effect(() => {
    const format = localConfig.teams.status_format;
    invoke<string>('preview_status', { format }).then((v) => {
      previewText = v;
    });
  });

  let unlistenFns: UnlistenFn[] = [];
  let unlistenAuth: (() => void) | null = null;

  onMount(async () => {
    await loadConfig();
    localConfig = JSON.parse(JSON.stringify($configStore));

    try {
      const syncStatus = await invoke<SyncStatus>('get_sync_status');
      isConnected = syncStatus.spotify_connected ?? false;
      teamsStatusConnected = syncStatus.teams_connected ?? false;
    } catch {
      // `get_sync_status` failure means the backend hasn't reported
      // connection state yet — default to disconnected and let the
      // auth-complete/failed listeners update these on first event.
      isConnected = false;
      teamsStatusConnected = false;
    }

    // Listen for reconnect-required events (emitted when backend clears tokens and needs re-auth)
    unlistenFns.push(await listen('spotify-reconnect-required', async () => {
      console.log('[SETTINGS] spotify-reconnect-required received');
      setSpotifyPhase('waiting');
      try {
        // The client_secret is no longer in the config — it lives in the OS
        // keychain (set during Onboarding). Re-auth needs the keychain
        // entry to still be present. If it isn't, redirect the user back
        // to Onboarding. See issue #9.
        const hasSecret = await invoke<boolean>('is_spotify_client_secret_set');
        if (!hasSecret) {
          console.warn('[SETTINGS] spotify-reconnect-required: keychain empty, redirecting to onboarding');
          currentView.set('onboarding');
          return;
        }
        await invoke('start_spotify_reconnect', {
          clientId: localConfig.spotify.client_id,
          redirectUri: 'presencejam://callback'
        });
      } catch (e) {
        console.error('[SETTINGS] start_spotify_reconnect failed:', e);
        setSpotifyPhase('error', String(e));
      }
    }));

    unlistenFns.push(await listen('teams-reconnect-required', async () => {
      console.log('[SETTINGS] teams-reconnect-required received');
      setTeamsPhase('waiting');
      try {
        await invoke('start_teams_auth_device_code');
      } catch (e) {
        console.error('[SETTINGS] start_teams_auth_device_code failed:', e);
        setTeamsPhase('error', String(e));
      }
    }));

    // Auth completion/failure events via the shared helper.
    unlistenAuth = await useAuthListeners({
      onSpotifyComplete: () => {
        console.log('[SETTINGS] spotify-auth-complete received');
        setSpotifyPhase('done');
        isConnected = true;
      },
      onSpotifyFailed: (payload) => {
        console.error('[SETTINGS] spotify-auth-failed:', payload);
        setSpotifyPhase('error', String(payload));
      },
      onTeamsComplete: () => {
        console.log('[SETTINGS] teams-auth-complete received');
        setTeamsPhase('done');
        teamsStatusConnected = true;
      },
      onTeamsFailed: (payload) => {
        console.error('[SETTINGS] teams-auth-failed:', payload);
        setTeamsPhase('error', String(payload));
      }
    });
  });

  onDestroy(() => {
    if (saveTimeout) {
      clearTimeout(saveTimeout);
      saveTimeout = null;
    }
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    if (unlistenAuth) unlistenAuth();
  });

  async function handleSave() {
    isSaving = true;
    saveMessage = '';
    try {
      await saveConfig(localConfig);
      saveMessage = 'Settings saved!';
      if (saveTimeout) clearTimeout(saveTimeout);
      saveTimeout = setTimeout(() => saveMessage = '', 2000);
    } catch {
      saveMessage = 'Failed to save';
    }
    isSaving = false;
  }

  async function openLogs() {
    await invoke('open_logs_folder');
  }

  async function reconnectSpotify() {
    if (spotifyAuthWaiting || !localConfig.spotify.client_id) return;
    setSpotifyPhase('waiting');
    try {
      await invoke('reconnect_spotify');
    } catch (e) {
      console.error('[SETTINGS] reconnect_spotify failed:', e);
      setSpotifyPhase('error', String(e));
    }
  }

  async function reconnectTeams() {
    if (teamsAuthWaiting) return;
    setTeamsPhase('waiting');
    try {
      await invoke('reconnect_teams');
    } catch (e) {
      console.error('[SETTINGS] reconnect_teams failed:', e);
      setTeamsPhase('error', String(e));
    }
  }

  function goBack() {
    currentView.set('dashboard');
  }

  function goToOnboarding() {
    // Used by the Spotify Client Secret hint when the keychain entry is
    // missing. Re-running Onboarding places a fresh secret in the keychain.
    // See issue #9.
    currentView.set('onboarding');
  }
</script>

<div class="settings">
  <PageHeader title="Settings" onBack={goBack} />

  <div class="sections">
    <section class="card">
      <header class="section-header">
        <h2>Spotify</h2>
        <span class="badge" class:success={isConnected && !spotifyAuthWaiting}
              class:warning={spotifyAuthWaiting}
              class:error={!isConnected && !spotifyAuthWaiting}>
          <span class="dot"></span>
          {#if spotifyAuthWaiting}Reconnecting…{:else if isConnected}Connected{:else}Not connected{/if}
        </span>
      </header>
      <div class="form-group">
        <label for="spotify-client-id">Client ID</label>
        <input
          id="spotify-client-id"
          type="text"
          bind:value={localConfig.spotify.client_id}
          readonly={isConnected}
          placeholder="Enter Spotify Client ID"
        />
      </div>
      <div class="form-group">
        <span class="form-label">Client secret</span>
        <p class="hint">
          {#if localConfig.spotify.client_secret_set}
            Stored securely in your operating system's keychain. To replace
            it, run Onboarding again.
          {:else}
            Not configured. <button type="button" class="btn-link" onclick={goToOnboarding}>Run Onboarding</button> to set up Spotify.
          {/if}
        </p>
      </div>
      <div class="connection-row">
        {#if isConnected && !spotifyAuthWaiting}
          <button class="btn-secondary" onclick={reconnectSpotify} disabled={spotifyAuthWaiting}>Reconnect Spotify</button>
        {:else if spotifyAuthWaiting}
          <span class="hint">Complete authentication in the browser.</span>
        {/if}
      </div>
    </section>

    <section class="card">
      <header class="section-header">
        <h2>Microsoft Teams</h2>
        <span class="badge" class:success={teamsStatusConnected && !teamsAuthWaiting}
              class:warning={teamsAuthWaiting}
              class:error={!teamsStatusConnected && !teamsAuthWaiting}>
          <span class="dot"></span>
          {#if teamsAuthWaiting}Reconnecting…{:else if teamsStatusConnected}Connected{:else}Not connected{/if}
        </span>
      </header>
      <p class="hint">Teams authentication uses your Microsoft 365 account. No additional configuration required.</p>
      <div class="connection-row">
        {#if teamsStatusConnected && !teamsAuthWaiting}
          <button class="btn-secondary" onclick={reconnectTeams} disabled={teamsAuthWaiting}>Reconnect Teams</button>
        {:else if teamsAuthWaiting}
          <span class="hint">Complete authentication in the browser.</span>
        {/if}
      </div>
    </section>

    <section class="card">
      <header class="section-header">
        <h2>Status format</h2>
      </header>
      <div class="form-group">
        <label for="status-format">Format template</label>
        <input
          id="status-format"
          type="text"
          bind:value={localConfig.teams.status_format}
          placeholder="🎵 {'{artist}'} - {'{track}'} 🎧"
        />
      </div>
      <div class="form-group">
        <span class="form-label">Live preview</span>
        <div class="preview-box" aria-live="polite">{previewText}</div>
      </div>
      <p class="hint">
        Available placeholders: <code>{'{artist}'}</code>, <code>{'{track}'}</code>,
        <code>{'{album}'}</code>, <code>{'{emoji}'}</code>
      </p>
      <div class="toggle-row">
        <label for="profanity-filter">Filter profanity in status</label>
        <input
          id="profanity-filter"
          type="checkbox"
          bind:checked={localConfig.teams.profanity_filter}
        />
      </div>
      {#if localConfig.teams.profanity_filter}
        <div class="form-group">
          <label for="profanity-placeholder">Placeholder text</label>
          <p class="hint">
            Use <code>{'{emoji}'}</code> for play state (🎵 playing / ⏸ paused).
            Shown when profanity is detected in track info.
          </p>
          <input
            id="profanity-placeholder"
            type="text"
            bind:value={localConfig.teams.profanity_placeholder}
            placeholder="Currently Listening to Spotify"
          />
        </div>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>Polling</h2>
      </header>
      <div class="form-group">
        <label for="default-interval">Default interval: {localConfig.polling.default_interval_seconds}s</label>
        <input
          id="default-interval"
          type="range"
          min="10"
          max="60"
          step="5"
          bind:value={localConfig.polling.default_interval_seconds}
        />
      </div>
      <div class="row-2">
        <div class="form-group">
          <label for="min-interval">Min interval (s)</label>
          <input
            id="min-interval"
            type="number"
            min="5"
            max="30"
            bind:value={localConfig.polling.minimum_interval_seconds}
          />
        </div>
        <div class="form-group">
          <label for="max-interval">Max interval (s)</label>
          <input
            id="max-interval"
            type="number"
            min="30"
            max="120"
            bind:value={localConfig.polling.max_interval_seconds}
          />
        </div>
      </div>
    </section>

    <section class="card">
      <header class="section-header">
        <h2>Appearance</h2>
      </header>
      <div class="form-group">
        <span class="form-label">Theme</span>
        <div class="theme-grid" role="radiogroup" aria-label="Theme">
          <button type="button" class="theme-card"
            class:is-active={$theme === 'dark'} aria-pressed={$theme === 'dark'}
            onclick={() => theme.set('dark')}>
            <span class="swatch swatch-dark"></span>
            <span class="theme-name">Dark</span>
          </button>
          <button type="button" class="theme-card"
            class:is-active={$theme === 'light'} aria-pressed={$theme === 'light'}
            onclick={() => theme.set('light')}>
            <span class="swatch swatch-light"></span>
            <span class="theme-name">Light</span>
          </button>
        </div>
      </div>
      <div class="toggle-row">
        <label for="autostart">Launch at login</label>
        <input
          id="autostart"
          type="checkbox"
          checked={localConfig.autostart}
          onchange={async (e) => {
            const enabled = (e.currentTarget as HTMLInputElement).checked;
            localConfig.autostart = enabled;
            await invoke('set_autostart_enabled', { enabled });
          }}
        />
      </div>
    </section>

    <section class="actions">
      <button class="btn-full" onclick={handleSave} disabled={isSaving}>
        {isSaving ? 'Saving…' : 'Save changes'}
      </button>
      {#if saveMessage}
        <p class="save-message" aria-live="polite">{saveMessage}</p>
      {/if}
      <button class="btn-secondary btn-full" onclick={openLogs}>Open logs folder</button>
    </section>
  </div>
</div>

<style>
  .settings {
    padding: var(--sp-5);
    max-width: 640px;
    margin: 0 auto;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }

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
    margin-bottom: var(--sp-1);
  }
  .section-header h2 {
    font-size: var(--fs-md);
    font-weight: 600;
  }

  .form-group { display: flex; flex-direction: column; gap: var(--sp-2); }
  .form-group label,
  .form-group .form-label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
  }
  .connection-row {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
  .connection-row .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }

  .preview-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-3) var(--sp-4);
    font-size: var(--fs-base);
    color: var(--fg);
    word-break: break-word;
    min-height: 40px;
  }

  .hint {
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
    line-height: var(--lh-normal);
  }
  .hint code {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    padding: 1px 6px;
    border-radius: var(--r-sm);
    font-size: var(--fs-xs);
  }

  .row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-3);
  }
  @media (max-width: 480px) {
    .row-2 { grid-template-columns: 1fr; }
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
  }
  .toggle-row label {
    font-size: var(--fs-base);
    color: var(--fg);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-3);
  }
  .theme-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--sp-2);
    padding: var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    cursor: pointer;
    width: auto;
    transition: border-color var(--dur-fast) var(--ease-out),
                background-color var(--dur-fast) var(--ease-out);
  }
  .theme-card:hover {
    background: var(--bg-surface);
    border-color: var(--border-strong);
  }
  .theme-card.is-active {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .swatch {
    display: block;
    height: 64px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
  }
  .swatch-dark { background: linear-gradient(135deg, #0F1226 0%, #232852 100%); }
  .swatch-light { background: linear-gradient(135deg, #F6F7FB 0%, #FFFFFF 100%); }
  .theme-name {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
    text-align: left;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    margin-top: var(--sp-3);
  }
  .btn-full {
    width: 100%;
    padding: var(--sp-3) var(--sp-5);
    font-size: var(--fs-md);
  }
  .btn-full.btn-secondary { background: var(--bg-elevated); }

  .save-message {
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--success);
    font-weight: 600;
  }
</style>
