<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-shell';
  import { configStore, saveConfig, type AppConfig } from '$lib/stores/config';

  let step = $state(1);
  let spotifyClientId = $state('');
  let spotifyClientSecret = $state('');
  let spotifyVerifier = $state('');
  let spotifyConnected = $state(false);
  let spotifyUsername = $state('');
  let spotifyManualUrl = $state('');
  let spotifyWaiting = $state(false);

  let teamsClientId = $state('');
  let teamsUserCode = $state('');
  let teamsVerificationUrl = $state('');
  let teamsDeviceCode = $state('');
  let teamsConnected = $state(false);
  let teamsUsername = $state('');
  let teamsPolling = $state(false);

  let statusFormat = $state('🎵 {artist} - {track} 🎧');
  let launchAtLogin = $state(false);
  let pollingInterval = $state(30);

  async function connectSpotify() {
    try {
      const verifier = crypto.randomUUID().replace(/-/g, '') + crypto.randomUUID().replace(/-/g, '');
      const challenge = btoa(String.fromCharCode(...new Uint8Array(
        await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))
      ))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
      
      const redirectUri = 'http://localhost:7890/callback';
      const authUrl = `https://accounts.spotify.com/authorize?client_id=${spotifyClientId}&response_type=code&redirect_uri=${encodeURIComponent(redirectUri)}&code_challenge_method=S256&code_challenge=${challenge}&scope=user-read-currently-playing+user-read-playback-state`;
      
      await open(authUrl);
      spotifyVerifier = verifier;
      spotifyWaiting = true;
      
      // Wait up to 5 minutes for callback server to receive the code
      const code = await waitForCallback();
      if (code) {
        await completeSpotifyAuth(code, verifier);
      }
    } catch (e) {
      console.error('Spotify auth failed:', e);
      spotifyWaiting = false;
    }
  }

  async function completeSpotifyAuth(code: string, verifier: string) {
    try {
      const tokens = await invoke<any>('complete_spotify_auth', {
        code,
        verifier,
        clientId: spotifyClientId,
        clientSecret: spotifyClientSecret
      });
      if (tokens) {
        spotifyConnected = true;
      }
    } catch (e) {
      console.error('Spotify token exchange failed:', e);
    } finally {
      spotifyWaiting = false;
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

  async function handleManualUrlPaste() {
    const code = extractCodeFromUrl(spotifyManualUrl);
    if (code && spotifyVerifier) {
      await completeSpotifyAuth(code, spotifyVerifier);
    }
  }

  async function waitForCallback(): Promise<string | null> {
    // Poll the local callback server every second for up to 5 minutes
    const timeout = Date.now() + 300000;
    while (Date.now() < timeout) {
      try {
        // Try fetching a simple endpoint - the server will capture the code from the redirect
        const resp = await fetch('http://localhost:7890/', { mode: 'no-cors' });
        // If we get here without error, server is running - wait a bit more for the redirect
        await new Promise(r => setTimeout(r, 2000));
      } catch {
        // Server not responding yet, keep waiting
      }
      await new Promise(r => setTimeout(r, 1000));
    }
    return null;
  }

  async function connectTeams() {
    try {
      const resp = await invoke<any>('start_teams_auth', { clientId: teamsClientId });
      teamsUserCode = resp.user_code;
      teamsVerificationUrl = resp.verification_url;
      teamsDeviceCode = resp.device_code;
      await open(resp.verification_url);
    } catch (e) {
      console.error('Teams auth failed:', e);
    }
  }

  async function pollTeamsAuth() {
    teamsPolling = true;
    try {
      const tokens = await invoke<any>('poll_teams_auth', {
        deviceCode: teamsDeviceCode,
        clientId: teamsClientId
      });
      if (tokens) {
        teamsConnected = true;
      }
    } catch (e) {
      console.error('Teams auth failed:', e);
    } finally {
      teamsPolling = false;
    }
  }

  async function finish() {
    const cfg: AppConfig = {
      spotify: {
        client_id: spotifyClientId,
        client_secret: spotifyClientSecret,
        redirect_uri: 'http://localhost:7890/callback',
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
        <label>Client ID</label>
        <input bind:value={spotifyClientId} placeholder="3abc..." />
      </div>
      <div class="form-group">
        <label>Client Secret</label>
        <input bind:value={spotifyClientSecret} type="password" placeholder="••••••••" />
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
          <button class="back" onclick={() => { spotifyWaiting = false; spotifyVerifier = ''; }}>
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
      
      {#if !teamsUserCode}
        <div class="form-group">
          <label>Client ID (from Azure AD app registration)</label>
          <input bind:value={teamsClientId} placeholder="abc123..." />
        </div>
        <button onclick={connectTeams} disabled={!teamsClientId}>
          Sign in with Microsoft
        </button>
      {:else if !teamsConnected}
        <div class="device-code-box">
          <p>Visit <a href={teamsVerificationUrl} onclick={(e) => { e.preventDefault(); open(teamsVerificationUrl); }}>{teamsVerificationUrl}</a></p>
          <div class="code-display">{teamsUserCode}</div>
          <p class="hint">Enter this code when prompted</p>
        </div>
        <button onclick={pollTeamsAuth} disabled={teamsPolling}>
          {teamsPolling ? 'Checking...' : "I've completed sign-in"}
        </button>
        {#if teamsPolling}
          <div class="spinner"></div>
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
        <label>Status format</label>
        <input bind:value={statusFormat} placeholder="🎵 {'{artist}'} - {'{track}'} 🎧" />
        <p class="hint">Use {'{artist}'}, {'{track}'}, {'{album}'}, {'{emoji}'}</p>
      </div>
      
      <div class="form-group">
        <label>Polling interval: {pollingInterval}s</label>
        <input type="range" min="10" max="60" step="5" bind:value={pollingInterval} />
      </div>
      
      <div class="form-group toggle">
        <label>Launch at login</label>
        <input type="checkbox" bind:checked={launchAtLogin} />
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
  .form-group label { display: block; margin-bottom: 6px; font-size: 13px; font-weight: 500; }
  input[type="text"],
  input[type="password"],
  input[type="email"] {
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
