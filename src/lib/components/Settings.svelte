<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, saveConfig, loadConfig, type AppConfig } from '$lib/stores/config';
  import type { SyncStatus, TeamsTokens } from '$lib/types';
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

  // Scopes granted on the stored Spotify access token (decoded backend-side
  // from the JWT payload). The tray playback feature needs
  // `user-modify-playback-state`, which existing users don't have until
  // they re-connect once — the banner below nudges them. Issue #3.0-P3.
  let grantedScopes = $state<string[]>([]);
  let playbackScopeMissing = $derived(
    isConnected && !grantedScopes.includes('user-modify-playback-state')
  );

  async function refreshGrantedScopes() {
    try {
      grantedScopes = await invoke<string[]>('get_spotify_granted_scopes');
    } catch (e) {
      console.error('[SETTINGS] get_spotify_granted_scopes failed:', e);
      grantedScopes = [];
    }
  }

  // Scopes granted on the stored Teams access token. The presence gate
  // needs `Presence.Read` and the availability sync's /users/{oid} fallback
  // needs the `profile` claim (oid) — existing users don't have them until
  // they re-connect once; the banner below nudges them. Issue #3.0-P1/P2.
  let teamsGrantedScopes = $state<string[]>([]);
  let teamsScopesMissing = $derived(
    teamsStatusConnected &&
      !(
        teamsGrantedScopes.includes('Presence.Read') &&
        teamsGrantedScopes.includes('profile')
      )
  );

  async function refreshTeamsGrantedScopes() {
    try {
      teamsGrantedScopes = await invoke<string[]>('get_teams_granted_scopes');
    } catch (e) {
      console.error('[SETTINGS] get_teams_granted_scopes failed:', e);
      teamsGrantedScopes = [];
    }
  }

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
    }).catch(()=>{ previewText='(preview unavailable)'; });
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

    // Detect whether the tray-playback scope is missing (existing users
    // re-auth with the new scope set; see issue #3.0-P3).
    await refreshGrantedScopes();
    // Detect whether the presence scopes are missing (existing users
    // re-auth with the new scope set; see issue #3.0-P1/P2).
    await refreshTeamsGrantedScopes();

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

    // NOTE: `teams-reconnect-required` is handled by the always-mounted
    // listener in +layout.svelte (issue #157). The polling loop can emit
    // it while Settings is not mounted (the normal Dashboard case), so a
    // Settings-only listener would drop the event. The layout listener
    // sets the authFlow phase, navigates to Settings, and starts the
    // device-code flow; Settings renders the code/URI from the store.

    // Auth completion/failure events via the shared helper.
    unlistenAuth = await useAuthListeners({
      onSpotifyComplete: () => {
        console.log('[SETTINGS] spotify-auth-complete received');
        setSpotifyPhase('done');
        isConnected = true;
        // The new token carries the freshly-granted scope set — refresh so
        // the playback banner disappears. Issue #3.0-P3.
        refreshGrantedScopes();
      },
      onSpotifyFailed: (payload) => {
        console.error('[SETTINGS] spotify-auth-failed:', payload);
        setSpotifyPhase('error', String(payload));
      },
      onTeamsComplete: () => {
        console.log('[SETTINGS] teams-auth-complete received');
        setTeamsPhase('done');
        teamsStatusConnected = true;
        // The new token carries the freshly-granted scope set — refresh so
        // the presence banner disappears. Issue #3.0-P1/P2.
        refreshTeamsGrantedScopes();
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
    } catch (e) { const msg = String((e as Error)?.message ?? e).slice(0, 180); saveMessage = msg || 'Failed to save'; console.error(e); }
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

  // Polls the backend for device-code completion. The cadence is
  // Rust-side; `interval` (from the DeviceCodeResponse stored in the
  // authFlow store) is threaded through so the server's requested
  // polling rate is honored — see issue #152.
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
        teamsStatusConnected = true;
      }
    } catch (e) {
      console.error('[SETTINGS] poll_teams_auth failed:', e);
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
      {#if playbackScopeMissing}
        <div class="scope-banner">
          <span class="hint">Playback control needs a one-time reconnect.</span>
          <button type="button" class="btn-link" onclick={reconnectSpotify} disabled={spotifyAuthWaiting}>Reconnect</button>
        </div>
      {/if}
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
          <div class="device-code-box">
            <p class="hint">Go to</p>
            <a class="verification-url" href={authFlow.teams.verificationUrl} target="_blank" rel="noopener">{authFlow.teams.verificationUrl}</a>
            <p class="hint">and enter this code</p>
            <div class="code-display" aria-live="polite">{authFlow.teams.userCode}</div>
            <div class="spinner" aria-hidden="true"></div>
            <p>Waiting for sign-in…</p>
            <button class="btn-secondary" onclick={pollTeamsAuth}>I've signed in — check now</button>
          </div>
          {#if authFlow.teams.error}
            <p class="error-message" role="alert">{authFlow.teams.error}</p>
          {/if}
        {:else}
          <button class="btn-secondary" onclick={reconnectTeams}>Reconnect Teams</button>
        {/if}
      </div>
      {#if teamsScopesMissing}
        <div class="scope-banner">
          <span class="hint">Presence features need a one-time Teams reconnect.</span>
          <button type="button" class="btn-link" onclick={reconnectTeams} disabled={teamsAuthWaiting}>Reconnect</button>
        </div>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>Presence</h2>
      </header>
      <div class="toggle-row">
        <label for="availability-sync">Show Available while listening</label>
        <input
          id="availability-sync"
          type="checkbox"
          bind:checked={localConfig.teams.availability_sync}
        />
      </div>
      <p class="hint">
        Off by default. Shows <em>Available</em> (not <em>Busy</em>) in
        Teams while a track plays, because setPresence only supports the
        Busy/InACall combination — see the setPresence limitation.
      </p>
      <div class="toggle-row">
        <label for="presence-gate">Pause status during meetings/calls/DND</label>
        <input
          id="presence-gate"
          type="checkbox"
          bind:checked={localConfig.teams.presence_gate}
        />
      </div>
      <p class="hint">
        On by default. Skips writing your Spotify status while Teams says
        you're busy, in a meeting, in a call, or presenting.
      </p>
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
  .connection-row .device-code-box {
    width: 100%;
  }

  /* One-time-reconnect banner for the missing tray-playback scope
     (issue #3.0-P3). */
  .scope-banner {
    margin-top: var(--sp-3);
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .scope-banner .hint { margin: 0; }

  /* Device-code box — mirrors Onboarding's (issue #157). */
  .device-code-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-4);
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
  }
  .device-code-box .hint { margin: 0; }
  .verification-url {
    display: inline-block;
    padding: var(--sp-2) var(--sp-4);
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: var(--r-md);
    font-weight: 600;
    word-break: break-all;
    text-decoration: none;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
  }
  .verification-url:hover { background: var(--bg-base); }
  .code-display {
    font-family: var(--font-mono);
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: 0.2em;
    color: var(--fg);
    background: var(--bg-base);
    border: 2px dashed var(--border-strong);
    border-radius: var(--r-md);
    padding: var(--sp-3);
    user-select: all;
    font-variant-numeric: tabular-nums;
  }
  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin: 0 auto;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error-message {
    color: var(--danger);
    font-size: var(--fs-sm);
    background: var(--danger-soft);
    border-radius: var(--r-md);
    padding: var(--sp-3);
    font-weight: 500;
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
