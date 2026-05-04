<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { configStore, saveConfig, type AppConfig } from '$lib/stores/config';
  import { currentView } from '$lib/stores/app';
  import { devLog } from '$lib/utils/dev';

  let step = $state(1);
  let spotifyClientId = $state('');
  let spotifyClientSecret = $state('');
  let spotifyConnected = $state(false);
  let spotifyUsername = $state('');
  let spotifyManualUrl = $state('');
  let spotifyWaiting = $state(false);
  let spotifyAuthError = $state('');

  let teamsUserCode = $state('');
  let teamsVerificationUrl = $state('');
  let teamsDeviceCode = $state('');
  let teamsConnected = $state(false);
  let teamsPolling = $state(false);
  let teamsAuthError = $state('');

  let statusFormat = $state('🎵 {artist} - {track} 🎧');
  let launchAtLogin = $state(false);
  let pollingInterval = $state(30);
  let validationError = $state('');
  let isFinishing = $state(false);

  let unlistenFns: (() => void)[] = [];

  onMount(async () => {
    devLog('[ONBOARDING] onMount: ENTRY');

    devLog('[ONBOARDING] onMount: setting up spotify-auth-complete listener');
    const fn1 = await listen('spotify-auth-complete', () => {
      devLog('[ONBOARDING] EVENT: spotify-auth-complete received');
      spotifyConnected = true;
      spotifyWaiting = false;
      spotifyAuthError = '';
      validationError = '';
      devLog('[ONBOARDING] EVENT: spotifyConnected=true, spotifyWaiting=false, spotifyAuthError=""');
    });

    devLog('[ONBOARDING] onMount: setting up spotify-auth-failed listener');
    const fn2 = await listen<string>('spotify-auth-failed', (event) => {
      console.error('[ONBOARDING] EVENT: spotify-auth-failed received:', event.payload);
      spotifyWaiting = false;
      spotifyAuthError = String(event.payload);
      devLog('[ONBOARDING] EVENT: spotifyWaiting=false, spotifyAuthError set');
    });

    devLog('[ONBOARDING] onMount: setting up teams-auth-complete listener');
    const fn3 = await listen('teams-auth-complete', () => {
      devLog('[ONBOARDING] EVENT: teams-auth-complete received');
      teamsConnected = true;
      teamsPolling = false;
      teamsAuthError = '';
      validationError = '';
      devLog('[ONBOARDING] EVENT: teamsConnected=true, teamsPolling=false, teamsAuthError=""');
    });

    devLog('[ONBOARDING] onMount: setting up teams-auth-failed listener');
    const fn4 = await listen<string>('teams-auth-failed', (event) => {
      console.error('[ONBOARDING] EVENT: teams-auth-failed received:', event.payload);
      teamsPolling = false;
      teamsAuthError = String(event.payload);
      devLog('[ONBOARDING] EVENT: teamsPolling=false, teamsAuthError set');
    });

    unlistenFns = [fn1, fn2, fn3, fn4];
    devLog('[ONBOARDING] onMount: listeners registered');
  });

  onDestroy(() => {
    devLog('[ONBOARDING] onDestroy: cleaning up listeners');
    unlistenFns.forEach(fn => fn());
    devLog('[ONBOARDING] onDestroy: listeners cleaned up');
  });

  async function connectSpotify() {
    validationError = '';

    // Input validation: spotifyClientId and spotifyClientSecret are required.
    // Spotify Client IDs are 32 hex characters (e.g. "3abc...def0").
    // Client secrets from the developer dashboard are typically 32+ chars.
    if (!spotifyClientId.trim()) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_id is empty');
      validationError = 'Spotify Client ID is required.';
      return;
    }
    if (!/^[A-Za-z0-9]{32}$/.test(spotifyClientId.trim())) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_id format invalid');
      validationError = 'Spotify Client ID must be exactly 32 hexadecimal characters.';
      return;
    }
    if (!spotifyClientSecret.trim()) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_secret is empty');
      validationError = 'Spotify Client Secret is required.';
      return;
    }
    if (spotifyClientSecret.trim().length < 32) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_secret too short');
      validationError = 'Spotify Client Secret appears to be invalid (too short — must be at least 32 characters).';
      return;
    }

    devLog('[ONBOARDING] connectSpotify: ENTRY');
    devLog('[ONBOARDING] connectSpotify: spotifyClientId.length=', spotifyClientId.length);
    devLog('[ONBOARDING] connectSpotify: redirectUri=presencejam://callback');
    
    try {
      devLog('[ONBOARDING] connectSpotify: calling invoke start_spotify_auth');
      await invoke('start_spotify_auth', {
        clientId: spotifyClientId,
        clientSecret: spotifyClientSecret,
        redirectUri: 'presencejam://callback'
      });
      devLog('[ONBOARDING] connectSpotify: invoke SUCCESS');
      spotifyWaiting = true;
      devLog('[ONBOARDING] connectSpotify: spotifyWaiting=true');
    } catch (e) {
      console.error('[ONBOARDING] connectSpotify: invoke FAILED:', e);
      spotifyWaiting = false;
      spotifyAuthError = String(e);
      devLog('[ONBOARDING] connectSpotify: spotifyWaiting=false (from error)');
    }
    
    devLog('[ONBOARDING] connectSpotify: EXIT');
  }

  async function handleManualUrlPaste() {
    devLog('[ONBOARDING] handleManualUrlPaste: ENTRY');
    devLog('[ONBOARDING] handleManualUrlPaste: spotifyManualUrl.length=', spotifyManualUrl.length);
    
    try {
      const code = extractCodeFromUrl(spotifyManualUrl);
      devLog('[ONBOARDING] handleManualUrlPaste: extracted code:', code ? 'present' : 'null');
      
      if (code) {
        devLog('[ONBOARDING] handleManualUrlPaste: calling invoke complete_spotify_auth_manual');
        const tokens = await invoke<any>('complete_spotify_auth_manual', { code });
        devLog('[ONBOARDING] handleManualUrlPaste: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');
        
        if (tokens) {
          spotifyConnected = true;
          spotifyWaiting = false;
          devLog('[ONBOARDING] handleManualUrlPaste: spotifyConnected=true, spotifyWaiting=false');
        }
      } else {
        devLog('[ONBOARDING] handleManualUrlPaste: no code extracted');
      }
    } catch (e) {
      console.error('[ONBOARDING] handleManualUrlPaste: FAILED:', e);
    }
    
    devLog('[ONBOARDING] handleManualUrlPaste: EXIT');
  }

  function extractCodeFromUrl(url: string): string | null {
    devLog('[ONBOARDING] extractCodeFromUrl: ENTRY - url.length=', url.length);
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      devLog('[ONBOARDING] extractCodeFromUrl: code=', code ? 'present' : 'null');
      if (!code) {
        devLog('[ONBOARDING] extractCodeFromUrl: no code in URL params');
        return null;
      }
      return code;
    } catch (e) {
      console.error('[ONBOARDING] extractCodeFromUrl: URL parse failed:', e);
      return null;
    }
  }

  async function connectTeams() {
    devLog('[ONBOARDING] connectTeams: ENTRY');
    
    try {
      devLog('[ONBOARDING] connectTeams: calling invoke start_teams_auth_device_code');
      const response = await invoke<any>('start_teams_auth_device_code');
      devLog('[ONBOARDING] connectTeams: invoke SUCCESS');
      devLog('[ONBOARDING] connectTeams: response.user_code=', response.user_code);
      devLog('[ONBOARDING] connectTeams: response.verification_url=', response.verification_url);
      devLog('[ONBOARDING] connectTeams: response.device_code=', response.device_code ? 'present' : 'null');
      
      teamsUserCode = response.user_code;
      teamsVerificationUrl = response.verification_url;
      teamsDeviceCode = response.device_code;
      devLog('[ONBOARDING] connectTeams: state updated');
      
      devLog('[ONBOARDING] connectTeams: calling invoke open_external_url');
      await invoke('open_external_url', { url: teamsVerificationUrl });
      devLog('[ONBOARDING] connectTeams: open_external_url SUCCESS');
    } catch (e) {
      console.error('[ONBOARDING] connectTeams: FAILED:', e);
      teamsAuthError = 'Failed to start Teams sign-in. Please try again.';
    }
    
    devLog('[ONBOARDING] connectTeams: EXIT');
  }

  async function pollTeamsAuth() {
    devLog('[ONBOARDING] pollTeamsAuth: ENTRY');
    teamsAuthError = '';
    
    try {
      teamsPolling = true;
      devLog('[ONBOARDING] pollTeamsAuth: teamsPolling=true');
      devLog('[ONBOARDING] pollTeamsAuth: calling invoke poll_teams_auth');
      devLog('[ONBOARDING] pollTeamsAuth: deviceCode.length=', teamsDeviceCode.length);
      
      const tokens = await invoke<any>('poll_teams_auth', { deviceCode: teamsDeviceCode });
      devLog('[ONBOARDING] pollTeamsAuth: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');
      
      if (tokens) {
        teamsConnected = true;
        teamsPolling = false;
        teamsAuthError = '';
        devLog('[ONBOARDING] pollTeamsAuth: teamsConnected=true, teamsPolling=false');
      }
    } catch (e) {
      console.error('[ONBOARDING] pollTeamsAuth: FAILED:', e);
      teamsAuthError = String(e);
      teamsPolling = false;
      devLog('[ONBOARDING] pollTeamsAuth: teamsAuthError set, teamsPolling=false');
    }
    
    devLog('[ONBOARDING] pollTeamsAuth: EXIT');
  }

  async function finish() {
    devLog('[ONBOARDING] finish: ENTRY');
    if (isFinishing) return;
    devLog('[ONBOARDING] finish: spotifyConnected=', spotifyConnected);
    devLog('[ONBOARDING] finish: teamsConnected=', teamsConnected);

    if (!spotifyConnected || !teamsConnected) {
      console.error('[ONBOARDING] finish: validation failed - spotifyConnected=', spotifyConnected, ', teamsConnected=', teamsConnected);
      validationError = 'Please connect both Spotify and Teams before finishing setup.';
      return;
    }

    isFinishing = true;
    try {
      devLog('[ONBOARDING] finish: step 1 - building config');
      const cfg: AppConfig = {
        spotify: {
          client_id: spotifyClientId,
          client_secret: spotifyClientSecret,
          redirect_uri: 'presencejam://callback',
          scopes: ['user-read-currently-playing', 'user-read-playback-state']
        },
        teams: {
          status_format: statusFormat,
          clear_on_pause: true,
          profanity_filter: true,
          profanity_placeholder: 'Currently Listening to Spotify',
          start_minimized: false
        },
        polling: {
          default_interval_seconds: pollingInterval,
          minimum_interval_seconds: 10,
          max_interval_seconds: 60,
          expiry_buffer_seconds: 10
        },
        logging: {
          enabled: true,
          log_level: 'Info',
          retention_days: 30
        },
        autostart: launchAtLogin
      };
      devLog('[ONBOARDING] finish: config built');

      devLog('[ONBOARDING] finish: step 2 - calling saveConfig');
      await saveConfig(cfg);
      devLog('[ONBOARDING] finish: saveConfig SUCCESS');

      devLog('[ONBOARDING] finish: step 3 - launchAtLogin=', launchAtLogin);
      if (launchAtLogin) {
        devLog('[ONBOARDING] finish: calling invoke set_autostart_enabled');
        try {
          await invoke('set_autostart_enabled', { enabled: true });
          devLog('[ONBOARDING] finish: set_autostart_enabled SUCCESS');
        } catch (e) {
          console.error('[ONBOARDING] finish: set_autostart_enabled FAILED (non-critical):', e);
        }
      }

      devLog('[ONBOARDING] finish: step 4 - calling invoke complete_onboarding');
      const result = await invoke('complete_onboarding');
      devLog('[ONBOARDING] finish: complete_onboarding SUCCESS, result=', result);

      devLog('[ONBOARDING] finish: step 5 - switching to dashboard');
      currentView.set('dashboard');
      devLog('[ONBOARDING] finish: currentView=dashboard');

      devLog('[ONBOARDING] finish: SUCCESS - all steps completed');
    } catch (e: unknown) {
      console.error('[ONBOARDING] finish: FAILED:', e);
      validationError = 'Setup failed: ' + (typeof e === 'string' ? e : (e as Error)?.message || String(e));
    } finally {
      isFinishing = false;
    }

    devLog('[ONBOARDING] finish: EXIT');
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

      <div class="instructions-box">
        <h3>Create a Spotify App</h3>
        <ol>
          <li>Go to <a href="https://developer.spotify.com/dashboard" target="_blank" rel="noopener">https://developer.spotify.com/dashboard</a></li>
          <li>Sign in and click <strong>Create App</strong></li>
          <li>Add this redirect URI: <code>presencejam://callback</code></li>
          <li>Fill in the app name and description, then save</li>
          <li>Copy your <strong>Client ID</strong> and <strong>Client Secret</strong> from the app settings</li>
        </ol>
      </div>

      <div class="form-group">
        <label for="spotify-client-id">Client ID</label>
        <input id="spotify-client-id" bind:value={spotifyClientId} placeholder="3abc..." />
      </div>
      <div class="form-group">
        <label for="spotify-client-secret">Client Secret</label>
        <input id="spotify-client-secret" bind:value={spotifyClientSecret} type="password" placeholder="••••••••" />
      </div>

      {#if validationError}
        <p class="error-message">{validationError}</p>
      {/if}

      {#if !spotifyConnected && !spotifyWaiting}
        <button onclick={connectSpotify} disabled={!spotifyClientId || !spotifyClientSecret}>
          Connect Spotify
        </button>
      {:else if spotifyWaiting}
        <div class="waiting-box">
          <div class="spinner"></div>
          <p>Spotify login opened in your browser.</p>
          <p class="hint">After you authorize, Spotify will redirect you to a URL. Paste that full URL in the field below.</p>
          <div class="form-group">
            <input 
              bind:value={spotifyManualUrl} 
              placeholder="Paste redirect URL here (e.g. presencejam://callback?code=***...)"
              onkeydown={(e) => e.key === 'Enter' && handleManualUrlPaste()}
            />
          </div>
          <button onclick={handleManualUrlPaste} disabled={!spotifyManualUrl}>
            Submit URL
          </button>
          <button class="back" onclick={() => { spotifyWaiting = false; }}>
            Cancel
          </button>
          {#if spotifyAuthError}
            <p class="error-message">{spotifyAuthError}</p>
          {/if}
        </div>
      {:else}
        <div class="success-badge">
          <span>✓</span> Connected to Spotify
        </div>
        <button onclick={() => { step = 2; devLog('[ONBOARDING] step changed to 2'); }}>Next →</button>
      {/if}
    </div>
  {:else if step === 2}
    <div class="step">
      <h2>Connect Microsoft Teams</h2>
      <p>Sign in with Microsoft to update your Teams presence.</p>

      {#if validationError}
        <p class="error-message">{validationError}</p>
      {/if}

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
        <button onclick={() => { step = 3; devLog('[ONBOARDING] step changed to 3'); }}>Next →</button>
      {/if}
      
      <button class="back" onclick={() => { step = 1; devLog('[ONBOARDING] step changed to 1'); }}>← Back</button>
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
      
      {#if validationError}
        <p class="error-message">{validationError}</p>
      {/if}
      
      <button onclick={finish} disabled={isFinishing}>Finish</button>
      <button class="back" onclick={() => { step = 2; devLog('[ONBOARDING] step changed to 2'); }}>← Back</button>
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
  .instructions-box {
    background: var(--bg-elevated);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 20px;
    font-size: 14px;
  }
  .instructions-box h3 {
    margin: 0 0 12px 0;
    font-size: 15px;
    font-weight: 600;
  }
  .instructions-box ol {
    margin: 0;
    padding-left: 20px;
  }
  .instructions-box li {
    margin-bottom: 8px;
    line-height: 1.5;
  }
  .instructions-box code {
    background: var(--bg-base);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 13px;
  }
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
