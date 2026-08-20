<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { configStore, saveConfig, type AppConfig } from '$lib/stores/config';
  import type { DeviceCodeResponse, SpotifyTokens, TeamsTokens } from '$lib/types';
  import { currentView } from '$lib/stores/app';
  import { authFlow, setSpotifyPhase, setTeamsPhase, setTeamsDeviceCode } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';
  import Logo from './Logo.svelte';

  let step = $state(1);
  let spotifyClientId = $state('');
  let spotifyClientSecret = $state('');
  let spotifyConnected = $derived(authFlow.spotify.phase === 'done');
  let spotifyUsername = $state('');
  let spotifyManualUrl = $state('');
  let spotifyWaiting = $derived(authFlow.spotify.phase === 'waiting');
  let spotifyAuthError = $derived(authFlow.spotify.error ?? '');

  // Device-code state lives in the authFlow store (set via
  // setTeamsDeviceCode) so Onboarding and Settings render the same
  // code/verification URI — see issue #157.
  let teamsUserCode = $derived(authFlow.teams.userCode);
  let teamsVerificationUrl = $derived(authFlow.teams.verificationUrl);
  let teamsDeviceCode = $derived(authFlow.teams.deviceCode);
  let teamsConnected = $derived(authFlow.teams.phase === 'done');
  let teamsPolling = $derived(authFlow.teams.phase === 'waiting');
  let teamsAuthError = $derived(authFlow.teams.error ?? '');

  let statusFormat = $state('🎵 {artist} - {track} 🎧');
  let launchAtLogin = $state(false);
  let pollingInterval = $state(30);
  let validationError = $state('');
  let isFinishing = $state(false);

  let unlisten: (() => void) | null = null;

  onMount(async () => {
    devLog('[ONBOARDING] onMount: ENTRY');

    unlisten = await useAuthListeners({
      onSpotifyComplete: () => {
        devLog('[ONBOARDING] EVENT: spotify-auth-complete received');
        setSpotifyPhase('done');
        validationError = '';
        devLog('[ONBOARDING] EVENT: setSpotifyPhase(done), validationError cleared');
      },
      onSpotifyFailed: (payload) => {
        console.error('[ONBOARDING] EVENT: spotify-auth-failed received:', payload);
        setSpotifyPhase('error', String(payload));
        devLog('[ONBOARDING] EVENT: setSpotifyPhase(error)');
      },
      onTeamsComplete: () => {
        devLog('[ONBOARDING] EVENT: teams-auth-complete received');
        setTeamsPhase('done');
        validationError = '';
        devLog('[ONBOARDING] EVENT: setTeamsPhase(done), validationError cleared');
      },
      onTeamsFailed: (payload) => {
        console.error('[ONBOARDING] EVENT: teams-auth-failed received:', payload);
        setTeamsPhase('error', String(payload));
        devLog('[ONBOARDING] EVENT: setTeamsPhase(error)');
      }
    });
    devLog('[ONBOARDING] onMount: listeners registered');
  });

  onDestroy(() => {
    devLog('[ONBOARDING] onDestroy: cleaning up listeners');
    if (unlisten) unlisten();
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
      setSpotifyPhase('waiting');
      devLog('[ONBOARDING] connectSpotify: setSpotifyPhase(waiting)');
    } catch (e) {
      console.error('[ONBOARDING] connectSpotify: invoke FAILED:', e);
      setSpotifyPhase('error', e instanceof Error ? e.message : String(e));
      devLog('[ONBOARDING] connectSpotify: setSpotifyPhase(error)');
    }

    devLog('[ONBOARDING] connectSpotify: EXIT');
  }

  async function handleManualUrlPaste() {
    devLog('[ONBOARDING] handleManualUrlPaste: ENTRY');
    devLog('[ONBOARDING] handleManualUrlPaste: spotifyManualUrl.length=', spotifyManualUrl.length);

    try {
      const extracted = extractCodeFromUrl(spotifyManualUrl);
      devLog('[ONBOARDING] handleManualUrlPaste: extracted code:', extracted ? 'present' : 'null');

      if (extracted) {
        devLog('[ONBOARDING] handleManualUrlPaste: calling invoke complete_spotify_auth_manual');
        // Pass the OAuth `state` through so the backend can validate it
        // against the stored value (CSRF check) — see issue #162.
        const tokens = await invoke<SpotifyTokens>('complete_spotify_auth_manual', {
          code: extracted.code,
          oauthState: extracted.state
        });
        devLog('[ONBOARDING] handleManualUrlPaste: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');

        if (tokens) {
          setSpotifyPhase('done');
          devLog('[ONBOARDING] handleManualUrlPaste: setSpotifyPhase(done)');
        }
      } else {
        devLog('[ONBOARDING] handleManualUrlPaste: no code extracted');
        validationError = 'No code found in URL — paste the full redirect URL with ?code=…';
      }
    } catch (e) {
      console.error('[ONBOARDING] handleManualUrlPaste: FAILED:', e);
      validationError = e instanceof Error ? e.message : String(e);
    }

    devLog('[ONBOARDING] handleManualUrlPaste: EXIT');
  }

  function extractCodeFromUrl(url: string): { code: string; state: string } | null {
    devLog('[ONBOARDING] extractCodeFromUrl: ENTRY - url.length=', url.length);
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      devLog('[ONBOARDING] extractCodeFromUrl: code=', code ? 'present' : 'null');
      if (!code) {
        devLog('[ONBOARDING] extractCodeFromUrl: no code in URL params');
        return null;
      }
      // The `state` param accompanies `code` in the redirect URL. A
      // missing state still passes (empty string) — the backend rejects
      // it, mirroring the deep-link path's CSRF check. See issue #162.
      return { code, state: parsed.searchParams.get('state') ?? '' };
    } catch (e) {
      console.error('[ONBOARDING] extractCodeFromUrl: URL parse failed:', e);
      return null;
    }
  }

  async function connectTeams() {
    devLog('[ONBOARDING] connectTeams: ENTRY');

    try {
      devLog('[ONBOARDING] connectTeams: calling invoke start_teams_auth_device_code');
      const response = await invoke<DeviceCodeResponse>('start_teams_auth_device_code');
      devLog('[ONBOARDING] connectTeams: invoke SUCCESS');
      devLog('[ONBOARDING] connectTeams: response.user_code=', response.user_code);
      devLog('[ONBOARDING] connectTeams: response.verification_url=', response.verification_url);
      devLog('[ONBOARDING] connectTeams: response.device_code=', response.device_code ? 'present' : 'null');

      // Store the DeviceCodeResponse so the polling cadence can honor
      // the server's `interval` (issue #152) and the Settings re-auth
      // path can render the same code/URI from the store (issue #157).
      setTeamsDeviceCode({
        userCode: response.user_code,
        verificationUrl: response.verification_url,
        deviceCode: response.device_code,
        interval: response.interval
      });
      devLog('[ONBOARDING] connectTeams: state updated');

      devLog('[ONBOARDING] connectTeams: calling invoke open_external_url');
      try {
        await invoke('open_external_url', { url: response.verification_url });
        devLog('[ONBOARDING] connectTeams: open_external_url SUCCESS');
      } catch (openErr) {
        console.warn('[ONBOARDING] connectTeams: open_external_url FAILED (non-fatal):', openErr);
        devLog('[ONBOARDING] connectTeams: open_external_url FAILED (non-fatal)');
      }

      // Auto-poll once the user opens the browser. The user can also retry manually.
      pollTeamsAuth();
    } catch (e) {
      console.error('[ONBOARDING] connectTeams: FAILED:', e);
      setTeamsPhase('error', String(e));
    }

    devLog('[ONBOARDING] connectTeams: EXIT');
  }

  async function pollTeamsAuth() {
    devLog('[ONBOARDING] pollTeamsAuth: ENTRY');
    setTeamsPhase('waiting');

    try {
      devLog('[ONBOARDING] pollTeamsAuth: setTeamsPhase(waiting)');
      devLog('[ONBOARDING] pollTeamsAuth: calling invoke poll_teams_auth');
      devLog('[ONBOARDING] pollTeamsAuth: deviceCode.length=', authFlow.teams.deviceCode.length);

      const tokens = await invoke<TeamsTokens>('poll_teams_auth', {
        deviceCode: authFlow.teams.deviceCode,
        interval: authFlow.teams.interval
      });
      devLog('[ONBOARDING] pollTeamsAuth: invoke SUCCESS, tokens=', tokens ? 'present' : 'null');

      if (tokens) {
        setTeamsPhase('done');
        devLog('[ONBOARDING] pollTeamsAuth: setTeamsPhase(done)');
      }
    } catch (e) {
      console.error('[ONBOARDING] pollTeamsAuth: FAILED:', e);
      setTeamsPhase('error', String(e));
      devLog('[ONBOARDING] pollTeamsAuth: setTeamsPhase(error)');
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
      // The Spotify client_secret is sent once to `start_spotify_auth`
      // (which writes it to the OS keychain) and is NOT included in the
      // config saved to disk. See issue #9.
      const cfg: AppConfig = {
        spotify: {
          client_id: spotifyClientId,
          client_secret_set: true,
          redirect_uri: 'presencejam://callback'
        },
        teams: {
          status_format: statusFormat,
          clear_on_pause: true,
          profanity_filter: true,
          profanity_placeholder: 'Currently Listening to Spotify',
          start_minimized: false,
          // P1/P2 defaults (mirror config.ts / config.rs): availability
          // sync OFF, presence gate ON. Issue #3.0-P1/P2.
          availability_sync: false,
          presence_gate: true
        },
        polling: {
          default_interval_seconds: pollingInterval,
          minimum_interval_seconds: 10,
          max_interval_seconds: 60,
          expiry_buffer_seconds: 10
        },
        logging: {
          enabled: true,
          log_level: 'Info'
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
  <header class="brand">
    <Logo size={32} withWordmark />
    <span class="step-label">Step {step} of 3</span>
  </header>

  <div class="progress" aria-hidden="true">
    <div class="step-dots">
      {#each [1, 2, 3] as n}
        <span class="dot" class:active={step === n} class:done={step > n}></span>
      {/each}
    </div>
    <div class="progress-track">
      <div class="progress-fill" style="width: {((step - 1) / 2) * 100}%"></div>
    </div>
  </div>

  <div class="step">
    {#if step === 1}
      <div class="card">
        <h2>Connect Spotify</h2>
        <p>
          Paste your Spotify application's <strong>Client ID</strong> and
          <strong>Client Secret</strong>. We'll start the sign-in flow once you
          click the button.
        </p>

        <div class="instructions-box">
          <h3>Get your Spotify credentials</h3>
          <ol>
            <li>Open the <a href="https://developer.spotify.com/dashboard" target="_blank" rel="noopener">Spotify developer dashboard</a> and create an app.</li>
            <li>Add <code>presencejam://callback</code> as a redirect URI.</li>
            <li>Copy the Client ID and Client Secret from the app's settings.</li>
          </ol>
        </div>

        <div class="form-group">
          <label for="client-id">Client ID</label>
          <input
            id="client-id"
            type="text"
            bind:value={spotifyClientId}
            placeholder="32-character Spotify Client ID"
            autocomplete="off"
            spellcheck="false"
          />
        </div>
        <div class="form-group">
          <label for="client-secret">Client Secret</label>
          <input
            id="client-secret"
            type="password"
            bind:value={spotifyClientSecret}
            placeholder="Spotify Client Secret"
            autocomplete="off"
            spellcheck="false"
          />
        </div>

        {#if validationError}
          <p class="error-message" role="alert">{validationError}</p>
        {/if}
        {#if spotifyAuthError}
          <p class="error-message" role="alert">{spotifyAuthError}</p>
        {/if}

        {#if !spotifyConnected && !spotifyWaiting}
          <button class="btn-full" onclick={connectSpotify}
            disabled={!spotifyClientId || !spotifyClientSecret}>
            Connect Spotify
          </button>
        {:else if spotifyWaiting}
          <div class="waiting-box">
            <div class="spinner" aria-hidden="true"></div>
            <p>Spotify sign-in is waiting…</p>
            <p class="hint">Complete the authorisation in your browser, or paste the redirect URL below.</p>
            <input
              type="text"
              bind:value={spotifyManualUrl}
              placeholder="presencejam://callback?code=…"
              onkeydown={(e) => e.key === 'Enter' && handleManualUrlPaste()}
            />
            <button class="btn-secondary" onclick={handleManualUrlPaste}>Submit code</button>
          </div>
        {:else}
          <div class="success-badge">
            <span aria-hidden="true">✓</span> Connected to Spotify
          </div>
          <button class="btn-full" onclick={() => { step = 2; devLog('[ONBOARDING] step changed to 2'); }}>Continue →</button>
        {/if}
      </div>
    {:else if step === 2}
      <div class="card">
        <h2>Sign in with Microsoft</h2>
        <p>
          We use Microsoft's device-code flow — a one-time <strong>code</strong>
          you enter at a Microsoft page. No extra setup required.
        </p>

        {#if !teamsConnected && !teamsPolling}
          <button class="btn-full" onclick={connectTeams}>Start Microsoft sign-in</button>
        {:else if teamsPolling}
          <div class="device-code-box">
            <p class="hint">Go to</p>
            <a class="verification-url" href={teamsVerificationUrl} target="_blank" rel="noopener">{teamsVerificationUrl}</a>
            <p class="hint">and enter this code</p>
            <div class="code-display" aria-live="polite">{teamsUserCode}</div>
            <div class="spinner" aria-hidden="true"></div>
            <p>Waiting for sign-in…</p>
            <button class="btn-secondary" onclick={pollTeamsAuth}>I've signed in — check now</button>
          </div>
        {:else}
          <div class="success-badge">
            <span aria-hidden="true">✓</span> Connected to Microsoft Teams
          </div>
          <button class="btn-full" onclick={() => { step = 3; devLog('[ONBOARDING] step changed to 3'); }}>Continue →</button>
        {/if}

        {#if teamsAuthError}
          <p class="error-message" role="alert">{teamsAuthError}</p>
        {/if}
      </div>
    {:else}
      <div class="card">
        <h2>Finishing touches</h2>
        <p>
          Choose how your status message should look and whether PresenceJam
          should launch when you sign in.
        </p>

        <div class="form-group">
          <label for="status-format-onb">Status template</label>
          <input
            id="status-format-onb"
            type="text"
            bind:value={statusFormat}
            placeholder="🎵 {'{artist}'} - {'{track}'} 🎧"
          />
          <p class="hint">
            Placeholders: <code>{'{artist}'}</code>, <code>{'{track}'}</code>,
            <code>{'{album}'}</code>, <code>{'{emoji}'}</code>
          </p>
        </div>

        <div class="form-group">
          <label for="poll-interval-onb">Default poll interval: {pollingInterval}s</label>
          <input id="poll-interval-onb" type="range" min="10" max="60" step="5" bind:value={pollingInterval} />
        </div>

        <div class="toggle-row">
          <label for="launch-at-login-onb">Launch at login</label>
          <input id="launch-at-login-onb" type="checkbox" bind:checked={launchAtLogin} />
        </div>

        {#if validationError}
          <p class="error-message" role="alert">{validationError}</p>
        {/if}

        <button class="btn-full" onclick={finish} disabled={isFinishing}>
          {isFinishing ? 'Setting up…' : 'Finish setup'}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .onboarding {
    padding: var(--sp-7) var(--sp-5);
    max-width: 480px;
    margin: 0 auto;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }

  .brand {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-3);
  }
  .step-label {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--fg-subtle);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 4px var(--sp-3);
    background: var(--bg-elevated);
    border-radius: var(--r-pill);
    border: 1px solid var(--border);
  }

  .progress {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .step-dots {
    display: flex;
    gap: var(--sp-2);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
    transition: background-color var(--dur) var(--ease-out),
                transform var(--dur) var(--ease-out);
  }
  .dot.active {
    background: var(--accent);
    transform: scale(1.4);
  }
  .dot.done { background: var(--success); }
  .progress-track {
    flex: 1;
    height: 2px;
    background: var(--border);
    border-radius: var(--r-pill);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: var(--r-pill);
    transition: width var(--dur-slow) var(--ease-out);
  }

  .step {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    padding: var(--sp-6);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    box-shadow: var(--shadow-2);
  }
  h2 {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  p { color: var(--fg-muted); font-size: var(--fs-base); }

  .instructions-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-4);
  }
  .instructions-box h3 {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
    margin-bottom: var(--sp-3);
  }
  .instructions-box ol {
    margin: 0;
    padding-left: var(--sp-5);
    color: var(--fg-muted);
    font-size: var(--fs-sm);
    line-height: var(--lh-normal);
  }
  .instructions-box li { margin-bottom: var(--sp-2); }
  .instructions-box li:last-child { margin-bottom: 0; }
  .instructions-box code {
    background: var(--bg-base);
    padding: 1px 6px;
    border-radius: var(--r-sm);
    font-size: 0.9em;
  }

  .device-code-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
  }
  .device-code-box .hint { margin: 0; font-size: var(--fs-sm); }
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
    font-size: var(--fs-3xl);
    font-weight: 700;
    letter-spacing: 0.2em;
    color: var(--fg);
    background: var(--bg-base);
    border: 2px dashed var(--border-strong);
    border-radius: var(--r-md);
    padding: var(--sp-4);
    user-select: all;
    font-variant-numeric: tabular-nums;
  }

  .waiting-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .waiting-box .hint { font-size: var(--fs-sm); margin: 0; }

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

  .success-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    color: var(--success);
    background: var(--success-soft);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    padding: var(--sp-3) var(--sp-4);
    font-weight: 600;
  }
  .success-badge span[aria-hidden] { font-size: var(--fs-lg); }

  .error-message {
    color: var(--danger);
    font-size: var(--fs-sm);
    background: var(--danger-soft);
    border-radius: var(--r-md);
    padding: var(--sp-3);
    font-weight: 500;
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-2) 0;
  }
  .toggle-row label { color: var(--fg); font-size: var(--fs-base); }

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
    font-size: 0.9em;
  }

  .btn-full {
    width: 100%;
    padding: var(--sp-3) var(--sp-5);
    font-size: var(--fs-md);
  }
  .btn-secondary { width: 100%; }
</style>
