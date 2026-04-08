<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { configStore, saveConfig, type AppConfig } from '$lib/stores/config';
  import { currentView } from '$lib/stores/app';

  let step = $state(1);
  let spotifyClientId = $state('');
  let spotifyClientSecret = $state('');
  let spotifyConnected = $state(false);
  let spotifyUsername = $state('');
  let spotifyManualUrl = $state('');
  let spotifyWaiting = $state(false);

  let teamsUserCode = $state('');
  let teamsVerificationUrl = $state('');
  let teamsDeviceCode = $state('');
  let teamsConnected = $state(false);
  let teamsPolling = $state(false);
  let teamsAuthError = $state('');

  let statusFormat = $state('🎵 {artist} - {track} 🎧');
  let launchAtLogin = $state(false);
  let pollingInterval = $state(30);

  onMount(() => {
    const unlistenComplete = listen('spotify-auth-complete', () => {
      spotifyConnected = true;
      spotifyWaiting = false;
    });
    
    const unlistenFailed = listen<string>('spotify-auth-failed', (event) => {
      console.error('Spotify auth failed:', event.payload);
      spotifyWaiting = false;
    });
    
    const unlistenTeamsComplete = listen('teams-auth-complete', () => {
      teamsConnected = true;
      teamsPolling = false;
    });
    
    const unlistenTeamsFailed = listen<string>('teams-auth-failed', (event) => {
      console.error('Teams auth failed:', event.payload);
      teamsPolling = false;
    });
    
    return () => {
      unlistenComplete.then(fn => fn());
      unlistenFailed.then(fn => fn());
      unlistenTeamsComplete.then(fn => fn());
      unlistenTeamsFailed.then(fn => fn());
    };
  });

  async function connectSpotify() {
    try {
      await invoke('start_spotify_auth', {
        clientId: spotifyClientId,
        clientSecret: spotifyClientSecret,
        redirectUri: 'presencejam://callback'
      });
      spotifyWaiting = true;
    } catch (e) {
      console.error('Spotify auth failed:', e);
    }
  }

  async function handleManualUrlPaste() {
    try {
      const code = extractCodeFromUrl(spotifyManualUrl);
      if (code) {
        const tokens = await invoke<any>('complete_spotify_auth_manual', { code });
        if (tokens) {
          spotifyConnected = true;
          spotifyWaiting = false;
        }
      }
    } catch (e) {
      console.error('Spotify token exchange failed:', e);
    }
  }

  function extractCodeFromUrl(url: string): string | null {
    try {
      const parsed = new URL(url);
      return parsed.searchParams.get('code');
    } catch {
      return null;
    }
  }

  async function connectTeams() {
    console.log('connectTeams START');
    try {
      console.log('connectTeams: calling invoke start_teams_auth_device_code');
      const response = await invoke<any>('start_teams_auth_device_code');
      console.log('connectTeams: got response', response);
      teamsUserCode = response.user_code;
      teamsVerificationUrl = response.verification_url;
      teamsDeviceCode = response.device_code;
      console.log('connectTeams: calling open_external_url');
      await invoke('open_external_url', { url: teamsVerificationUrl });
      console.log('connectTeams: done');
    } catch (e) {
      console.error('connectTeams ERROR:', e);
    }
  }

  async function pollTeamsAuth() {
    teamsAuthError = '';
    try {
      teamsPolling = true;
      const tokens = await invoke<any>('poll_teams_auth', { deviceCode: teamsDeviceCode });
      if (tokens) {
        teamsConnected = true;
        teamsPolling = false;
        teamsAuthError = '';
      }
    } catch (e) {
      console.error('Teams auth failed:', e);
      teamsAuthError = String(e);
      teamsPolling = false;
    }
  }

  async function finish() {
    try {
      const cfg: AppConfig = {
        spotify: {
          client_id: spotifyClientId,
          client_secret: spotifyClientSecret,
          redirect_uri: 'presencejam://callback',
          scopes: ['user-read-currently-playing', 'user-read-playback-state']
        },
        teams: {
          status_format: statusFormat,
          clear_on_pause: true
        },
        polling: {
          default_interval_seconds: pollingInterval,
          minimum_interval_seconds: 5,
          max_interval_seconds: 10,
          expiry_buffer_seconds: 10
        },
        logging: {
          enabled: true,
          log_level: 'Info',
          retention_days: 30
        }
      };
      await saveConfig(cfg);
      
      if (launchAtLogin) {
        await invoke('set_autostart_enabled', { enabled: true });
      }
      
      await invoke('complete_onboarding');
      
      currentView.set('dashboard');
    } catch (e) {
      console.error('Finish failed:', e);
    }
  }
</script>

<div class="onboarding">
  <div class="progress">
    <div class="step-dots">
      {#each [1,2,3] as s}
        <div class="dot" class:active={s <= step} class:done={s < step}></div>
      {/each}
    </div>
    <span class="step-label">Step {step} of 3</span>
  </div>

  {#if step === 1}
    <div class="step">
      <h2>Let's get started</h2>
      <p>PresenceJam syncs your Spotify playback to your Teams status.</p>
      
      <div class="form-group">
        <label for="spotify-client-id">Client ID</label>
        <input id="spotify-client-id" bind:value={spotifyClientId} placeholder="3abc..." />
      </div>
      <div class="form-group">
        <label for="spotify-client-secret">Client Secret</label>
        <input id="spotify-client-secret" bind:value={spotifyClientSecret} type="password" placeholder="••••••••" />
      </div>
      
      {#if !spotifyConnected && !spotifyWaiting}
        <button onclick={connectSpotify} disabled={!spotifyClientId || !spotifyClientSecret}>
          Connect Spotify
        </button>
      {:else if spotifyWaiting}
        <div class="waiting-box">
          <div class="spinner"></div>
          <p>Spotify login opened in your browser.</p>
          <p class="hint">After you authorize, paste the full redirect URL below if the app doesn't detect it automatically.</p>
          <div class="form-group">
            <input 
              bind:value={spotifyManualUrl} 
              placeholder="Paste redirect URL here (e.g. http://localhost:7890/callback?code=XXX...)"
              onkeydown={(e) => e.key === 'Enter' && handleManualUrlPaste()}
            />
          </div>
          <button onclick={handleManualUrlPaste} disabled={!spotifyManualUrl}>
            Submit URL
          </button>
          <button class="back" onclick={() => { spotifyWaiting = false; }}>
            Cancel
          </button>
        </div>
      {:else}
        <div class="success-badge">
          <span>✓</span> Connected to Spotify
        </div>
        <button onclick={() => step = 2}>Next →</button>
      {/if}
    </div>
  {:else if step === 2}
    <div class="step">
      <h2>Connect Microsoft Teams</h2>
      <p>Sign in with Microsoft to update your Teams presence.</p>
      
      {#if !teamsConnected && !teamsUserCode}
        <button onclick={() => connectTeams()}>
          Sign in with Microsoft
        </button>
      {:else if !teamsConnected}
        <div class="device-code-box">
          <p>Visit <a href={teamsVerificationUrl}>{teamsVerificationUrl}</a></p>
          <div class="code-display">{teamsUserCode}</div>
          <p class="hint">Enter this code when prompted</p>
        </div>
        <button onclick={pollTeamsAuth} disabled={teamsPolling}>
          {teamsPolling ? 'Checking...' : "I've completed sign-in"}
        </button>
        {#if teamsPolling}
          <div class="spinner"></div>
        {/if}
        {#if teamsAuthError}
          <p class="error-message">{teamsAuthError}</p>
        {/if}
      {:else}
        <div class="success-badge">
          <span>✓</span> Connected to Microsoft Teams
        </div>
        <button onclick={() => step = 3}>Next →</button>
      {/if}
      
      <button class="back" onclick={() => step = 1}>← Back</button>
    </div>
  {:else}
    <div class="step">
      <h2>Customize your status</h2>
      
      <div class="form-group">
        <label for="status-format">Status format</label>
        <input id="status-format" bind:value={statusFormat} placeholder="🎵 {'{artist}'} - {'{track}'} 🎧" />
        <p class="hint">Use {'{artist}'}, {'{track}'}, {'{album}'}, {'{emoji}'}</p>
      </div>
      
      <div class="form-group">
        <label for="polling-interval">Polling interval: {pollingInterval}s</label>
        <input id="polling-interval" type="range" min="10" max="60" step="5" bind:value={pollingInterval} />
      </div>
      
      <div class="form-group toggle">
        <label for="launch-at-login">Launch at login</label>
        <input id="launch-at-login" type="checkbox" bind:checked={launchAtLogin} />
      </div>
      
      <button onclick={finish}>Finish</button>
      <button class="back" onclick={() => step = 2}>← Back</button>
    </div>
  {/if}
</div>

<style>
  .onboarding {
    padding: 32px;
    max-width: 420px;
    margin: 0 auto;
    height: 100vh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    overflow: hidden;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 32px;
  }
  .step-dots {
    display: flex;
    gap: 8px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--border-color);
    transition: all 0.3s;
  }
  .dot.active { background: var(--color-accent); }
  .dot.done { background: var(--color-success); }
  .step-label { font-size: 12px; color: var(--text-secondary); }
  h2 { font-size: 24px; margin-bottom: 8px; }
  p { color: var(--text-secondary); margin-bottom: 24px; font-size: 14px; }
  .form-group { margin-bottom: 16px; }
  .step {
    flex: 1;
    overflow-y: auto;
  }
  .form-group label { display: block; margin-bottom: 6px; font-size: 13px; font-weight: 500; }
  input[type="password"],
  input[type="range"] {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 14px;
    box-sizing: border-box;
  }
  input:focus {
    outline: none;
    border-color: var(--color-accent);
  }
  input[type="range"] {
    width: 100%;
    margin-top: 8px;
  }
  input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: var(--color-accent);
  }
  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .toggle label {
    margin-bottom: 0;
  }
  button {
    width: 100%;
    padding: 12px 16px;
    background: var(--color-accent);
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.2s;
  }
  button:hover:not(:disabled) {
    opacity: 0.9;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .hint { font-size: 12px; color: var(--text-secondary); margin-top: 4px; }
  .back { background: transparent; border: 1px solid var(--border-color); margin-top: 12px; color: var(--text-secondary); }
  .back:hover { background: var(--bg-elevated); }
  .success-badge { display: flex; align-items: center; gap: 8px; color: var(--color-success); margin-bottom: 16px; font-weight: 500; }
  .error-message { color: var(--color-error); font-size: 13px; margin-top: 8px; padding: 8px; background: rgba(255,0,0,0.1); border-radius: 4px; }
  .device-code-box { background: var(--bg-elevated); border-radius: 8px; padding: 16px; margin-bottom: 16px; }
  .code-display { font-size: 32px; font-weight: 700; letter-spacing: 4px; color: var(--color-accent); text-align: center; padding: 16px 0; }
  a { color: var(--color-accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .waiting-box {
    background: var(--bg-elevated);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 16px;
    text-align: center;
  }
  .waiting-box p { margin-bottom: 12px; }
  .waiting-box .hint { font-size: 12px; color: var(--text-secondary); margin-bottom: 16px; }
  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-color);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin: 0 auto 16px;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
