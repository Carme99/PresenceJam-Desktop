<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
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
  let validationError = $state('');
  let isFinishing = $state(false);

  let unlistenFns: (() => void)[] = [];

  onMount(async () => {
    console.log('[ONBOARDING] onMount: ENTRY');

    console.log('[ONBOARDING] onMount: setting up spotify-auth-complete listener');
    const unlistenComplete = listen('spotify-auth-complete', () => {
      console.log('[ONBOARDING] EVENT: spotify-auth-complete received');
      spotifyConnected = true;
      spotifyWaiting = false;
      validationError = '';
      console.log('[ONBOARDING] EVENT: spotifyConnected=true, spotifyWaiting=false');
    });

    console.log('[ONBOARDING] onMount: setting up spotify-auth-failed listener');
    const unlistenFailed = listen<string>('spotify-auth-failed', (event) => {
      console.error('[ONBOARDING] EVENT: spotify-auth-failed received:', event.payload);
      spotifyWaiting = false;
      console.log('[ONBOARDING] EVENT: spotifyWaiting=false (from failed)');
    });

    console.log('[ONBOARDING] onMount: setting up teams-auth-complete listener');
    const unlistenTeamsComplete = listen('teams-auth-complete', () => {
      console.log('[ONBOARDING] EVENT: teams-auth-complete received');
      teamsConnected = true;
      teamsPolling = false;
      teamsAuthError = '';
      validationError = '';
      console.log('[ONBOARDING] EVENT: teamsConnected=true, teamsPolling=false, teamsAuthError=""');
    });

    console.log('[ONBOARDING] onMount: setting up teams-auth-failed listener');
    const unlistenTeamsFailed = listen<string>('teams-auth-failed', (event) => {
      console.error('[ONBOARDING] EVENT: teams-auth-failed received:', event.payload);
      teamsPolling = false;
      teamsAuthError = String(event.payload);
      console.log('[ONBOARDING] EVENT: teamsPolling=false, teamsAuthError set');
    });

    const [fn1, fn2, fn3, fn4] = await Promise.all([unlistenComplete, unlistenFailed, unlistenTeamsComplete, unlistenTeamsFailed]);
    unlistenFns = [fn1, fn2, fn3, fn4];
    console.log('[ONBOARDING] onMount: listeners registered');
  });

  onDestroy(() => {
    console.log('[ONBOARDING] onDestroy: cleaning up listeners');
    unlistenFns.forEach(fn => fn());
    console.log('[ONBOARDING] onDestroy: listeners cleaned up');
  });

  async function connectSpotify() {
    console.log('[ONBOARDING] connectSpotify: ENTRY');
    console.log('[ONBOARDING] connectSpotify: spotifyClientId.length=', spotifyClientId.length);
    console.log('[ONBOARDING] connectSpotify: spotifyClientSecret.length=', spotifyClientSecret.length);
    console.log('[ONBOARDING] connectSpotify: redirectUri=presencejam://callback');
    
    try {
      console.log('[ONBOARDING] connectSpotify: calling invoke start_spotify_auth');
      await invoke('start_spotify_auth', {
        clientId: spotifyClientId,
        clientSecret: spotifyClientSecret,
        redirectUri: 'presencejam://callback'
      });
      console.log('[ONBOARDING] connectSpotify: invoke SUCCESS');
      spotifyWaiting = true;
      console.log('[ONBOARDING] connectSpotify: spotifyWaiting=true');
    } catch (e) {
      console.error('[ONBOARDING] connectSpotify: invoke FAILED:', e);
      spotifyWaiting = false;
      console.log('[ONBOARDING] connectSpotify: spotifyWaiting=false (from error)');
    }
    
    console.log('[ONBOARDING] connectSpotify: EXIT');
  }

  async function handleManualUrlPaste() {
    console.log('[ONBOARDING] handleManualUrlPaste: ENTRY');
    console.log('[ONBOARDING] handleManualUrlPaste: spotifyManualUrl.length=', spotifyManualUrl.length);
    
    try {
      const code = extractCodeFromUrl(spotifyManualUrl);
      console.log('[ONBOARDING] handleManualUrlPaste: extracted code:', code ? 'present' : 'null');
      
      if (code) {
        console.log('[ONBOARDING] handleManualUrlPaste: calling invoke complete_spotify_auth_manual');
        const tokens = await invoke<any>('complete_spotify_auth_manual', { code });
        console.log('[ONBOARDING] handleManualUrlPaste: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');
        
        if (tokens) {
          spotifyConnected = true;
          spotifyWaiting = false;
          console.log('[ONBOARDING] handleManualUrlPaste: spotifyConnected=true, spotifyWaiting=false');
        }
      } else {
        console.log('[ONBOARDING] handleManualUrlPaste: no code extracted');
      }
    } catch (e) {
      console.error('[ONBOARDING] handleManualUrlPaste: FAILED:', e);
    }
    
    console.log('[ONBOARDING] handleManualUrlPaste: EXIT');
  }

  function extractCodeFromUrl(url: string): string | null {
    console.log('[ONBOARDING] extractCodeFromUrl: ENTRY - url.length=', url.length);
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      console.log('[ONBOARDING] extractCodeFromUrl: code=', code ? 'present' : 'null');
      if (!code) {
        console.log('[ONBOARDING] extractCodeFromUrl: no code in URL params');
        return null;
      }
      return code;
    } catch (e) {
      console.error('[ONBOARDING] extractCodeFromUrl: URL parse failed:', e);
      return null;
    }
  }

  async function connectTeams() {
    console.log('[ONBOARDING] connectTeams: ENTRY');
    
    try {
      console.log('[ONBOARDING] connectTeams: calling invoke start_teams_auth_device_code');
      const response = await invoke<any>('start_teams_auth_device_code');
      console.log('[ONBOARDING] connectTeams: invoke SUCCESS');
      console.log('[ONBOARDING] connectTeams: response.user_code=', response.user_code);
      console.log('[ONBOARDING] connectTeams: response.verification_url=', response.verification_url);
      console.log('[ONBOARDING] connectTeams: response.device_code=', response.device_code ? 'present' : 'null');
      
      teamsUserCode = response.user_code;
      teamsVerificationUrl = response.verification_url;
      teamsDeviceCode = response.device_code;
      console.log('[ONBOARDING] connectTeams: state updated');
      
      console.log('[ONBOARDING] connectTeams: calling invoke open_external_url');
      await invoke('open_external_url', { url: teamsVerificationUrl });
      console.log('[ONBOARDING] connectTeams: open_external_url SUCCESS');
    } catch (e) {
      console.error('[ONBOARDING] connectTeams: FAILED:', e);
    }
    
    console.log('[ONBOARDING] connectTeams: EXIT');
  }

  async function pollTeamsAuth() {
    console.log('[ONBOARDING] pollTeamsAuth: ENTRY');
    teamsAuthError = '';
    
    try {
      teamsPolling = true;
      console.log('[ONBOARDING] pollTeamsAuth: teamsPolling=true');
      console.log('[ONBOARDING] pollTeamsAuth: calling invoke poll_teams_auth');
      console.log('[ONBOARDING] pollTeamsAuth: deviceCode.length=', teamsDeviceCode.length);
      
      const tokens = await invoke<any>('poll_teams_auth', { deviceCode: teamsDeviceCode });
      console.log('[ONBOARDING] pollTeamsAuth: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');
      
      if (tokens) {
        teamsConnected = true;
        teamsPolling = false;
        teamsAuthError = '';
        console.log('[ONBOARDING] pollTeamsAuth: teamsConnected=true, teamsPolling=false');
      }
    } catch (e) {
      console.error('[ONBOARDING] pollTeamsAuth: FAILED:', e);
      teamsAuthError = String(e);
      teamsPolling = false;
      console.log('[ONBOARDING] pollTeamsAuth: teamsAuthError set, teamsPolling=false');
    }
    
    console.log('[ONBOARDING] pollTeamsAuth: EXIT');
  }

  async function finish() {
    console.log('[ONBOARDING] finish: ENTRY');
    if (isFinishing) return;
    console.log('[ONBOARDING] finish: spotifyConnected=', spotifyConnected);
    console.log('[ONBOARDING] finish: teamsConnected=', teamsConnected);

    if (!spotifyConnected || !teamsConnected) {
      console.error('[ONBOARDING] finish: validation failed - spotifyConnected=', spotifyConnected, ', teamsConnected=', teamsConnected);
      validationError = 'Please connect both Spotify and Teams before finishing setup.';
      return;
    }

    isFinishing = true;
    try {
      console.log('[ONBOARDING] finish: step 1 - building config');
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
          launch_at_login: launchAtLogin,
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
        }
      };
      console.log('[ONBOARDING] finish: config built');

      console.log('[ONBOARDING] finish: step 2 - calling saveConfig');
      await saveConfig(cfg);
      console.log('[ONBOARDING] finish: saveConfig SUCCESS');

      console.log('[ONBOARDING] finish: step 3 - launchAtLogin=', launchAtLogin);
      if (launchAtLogin) {
        console.log('[ONBOARDING] finish: calling invoke set_autostart_enabled');
        try {
          await invoke('set_autostart_enabled', { enabled: true });
          console.log('[ONBOARDING] finish: set_autostart_enabled SUCCESS');
        } catch (e) {
          console.error('[ONBOARDING] finish: set_autostart_enabled FAILED (non-critical):', e);
        }
      }

      console.log('[ONBOARDING] finish: step 4 - calling invoke complete_onboarding');
      const result = await invoke('complete_onboarding');
      console.log('[ONBOARDING] finish: complete_onboarding SUCCESS, result=', result);

      console.log('[ONBOARDING] finish: step 5 - switching to dashboard');
      currentView.set('dashboard');
      console.log('[ONBOARDING] finish: currentView=dashboard');

      console.log('[ONBOARDING] finish: SUCCESS - all steps completed');
    } catch (e: unknown) {
      console.error('[ONBOARDING] finish: FAILED:', e);
      validationError = 'Setup failed: ' + (typeof e === 'string' ? e : (e as Error)?.message || String(e));
    } finally {
      isFinishing = false;
    }

    console.log('[ONBOARDING] finish: EXIT');
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
        <button onclick={() => { step = 2; console.log('[ONBOARDING] step changed to 2'); }}>Next →</button>
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
        <button onclick={() => { step = 3; console.log('[ONBOARDING] step changed to 3'); }}>Next →</button>
      {/if}
      
      <button class="back" onclick={() => { step = 1; console.log('[ONBOARDING] step changed to 1'); }}>← Back</button>
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
      <button class="back" onclick={() => { step = 2; console.log('[ONBOARDING] step changed to 2'); }}>← Back</button>
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
