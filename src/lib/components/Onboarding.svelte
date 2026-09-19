<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { loadConfig, mergeWizardConfig, saveConfig } from '$lib/stores/config';
  import type { AppConfig, DeviceCodeResponse } from '$lib/types';
  import { currentView } from '$lib/stores/app';
  import { authFlow, setSpotifyPhase, setTeamsPhase, setTeamsDeviceCode, expiresAtFromResponse, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, pollTeamsAuth } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';
  import Logo from './Logo.svelte';
  import DeviceCodeBox from './DeviceCodeBox.svelte';
  import { t } from '$lib/i18n';

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

  // Device-code expiry countdown (issue #429). The 1s ticker only runs
  // while a live code is waiting; $effect cleanup clears the interval on
  // unmount, independent of the onMount/onDestroy listener guard (#392).
  let expiryNow = $state(Date.now());
  let teamsRemainingMs = $derived(
    authFlow.teams.expiresAt == null ? null : authFlow.teams.expiresAt - expiryNow
  );
  let teamsCodeExpired = $derived(teamsRemainingMs != null && teamsRemainingMs <= 0);
  $effect(() => {
    if (authFlow.teams.phase !== 'waiting' || authFlow.teams.expiresAt == null || teamsCodeExpired) return;
    expiryNow = Date.now();
    const id = setInterval(() => { expiryNow = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  let statusFormat = $state('🎵 {artist} - {track} 🎧');
  let launchAtLogin = $state(false);
  let pollingInterval = $state(30);
  // #545: in-flight guard for the manual-URL submit. Every other flow entry
  // point in this component sets its flag before the first await (#394); a
  // double-click here used to consume an already-used authorization code.
  let manualSubmitBusy = $state(false);
  let validationError = $state('');
  let isFinishing = $state(false);
  // #967: whether this install is already configured. The wizard is reachable
  // by returning users (Settings' "Run onboarding", Dashboard's goToSetup, the
  // boot probe's fail-open path), and their only exit today was to complete
  // both sign-ins or quit the app — with a Spotify client secret they cannot
  // read back, that is a dead end. Probed from `is_onboarding_complete()`, the
  // same verdict the boot gate uses, so a first-run install (nothing to go
  // back to) keeps its one-way flow.
  let alreadyComplete = $state(false);
  // #394: in-flight guards — set BEFORE the first await so a double-click
  // cannot start two flows. Mirrors Settings.svelte / Reconnect.svelte.
  let spotifyConnecting = $state(false);
  let teamsConnecting = $state(false);
  // #615: `useAuthListeners` now returns its teardown synchronously and
  // handles the #392 unmount-while-registering race internally, so the
  // call site only holds the handle (no `destroyed` flag).
  let unlistenAuth: (() => Promise<void>) | null = null;
  // #387: after a step change the removed Continue button drops focus to
  // <body> — move it to the new step heading (tabindex -1, no visual change).
  function focusStepHeading() {
    requestAnimationFrame(() => {
      document.getElementById('onboarding-step-heading')?.focus();
    });
  }

  function goToDashboard() {
    devLog('[ONBOARDING] goToDashboard: leaving the wizard for the dashboard');
    currentView.set('dashboard');
  }

  onMount(() => {
    devLog('[ONBOARDING] onMount: ENTRY');
    unlistenAuth = useAuthListeners({
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

    // #531/#542: the wizard is reachable by RETURNING users (Dashboard's
    // goToSetup, Settings' "Run onboarding", Reconnect), so it must show
    // their stored settings rather than its own defaults — and `finish()`
    // merges into that same stored config instead of replacing it. The
    // store load also warms `configStore` for the views that follow.
    // The prefill is awaited inside an IIFE rather than by making the
    // `onMount` callback `async`: Svelte treats an async callback's returned
    // promise as a teardown function (#392), and the listener registration
    // above must stay synchronous.
    void (async () => {
      try {
        const loaded = await loadConfig();
        statusFormat = loaded.teams.status_format;
        launchAtLogin = loaded.autostart;
        pollingInterval = Number(loaded.polling.default_interval_seconds);
        spotifyClientId = loaded.spotify.client_id;
        devLog('[ONBOARDING] onMount: prefilled from stored config');
      } catch (e) {
        // `loadConfig` already falls back to `defaultConfig` and never
        // rejects; this guard only keeps a future change from breaking the
        // wizard silently. Prefill is display-only — `finish()` re-reads.
        console.warn('[ONBOARDING] onMount: config prefill failed:', e);
      }

      // #967: separate from the config prefill on purpose — a failed probe
      // must leave the wizard one-way rather than offer a way out of
      // first-run setup.
      try {
        alreadyComplete = await invoke<boolean>('is_onboarding_complete');
        devLog('[ONBOARDING] onMount: is_onboarding_complete=', alreadyComplete);
      } catch (e) {
        console.warn('[ONBOARDING] onMount: onboarding-complete probe failed:', e);
        alreadyComplete = false;
      }
    })();
  });

  onDestroy(() => {
    devLog('[ONBOARDING] onDestroy: cleaning up listeners');
    if (unlistenAuth) void unlistenAuth();
    devLog('[ONBOARDING] onDestroy: listeners cleaned up');
  });

  async function connectSpotify() {
    validationError = '';

    // Input validation: spotifyClientId and spotifyClientSecret are required.
    // Spotify Client IDs are 32 hex characters (e.g. "3abc...def0").
    // Client secrets from the developer dashboard are typically 32+ chars.
    if (!spotifyClientId.trim()) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_id is empty');
      validationError = t('validation.clientIdRequired');
      return;
    }
    if (!/^[A-Za-z0-9]{32}$/.test(spotifyClientId.trim())) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_id format invalid');
      validationError = t('validation.clientIdFormat');
      return;
    }
    if (!spotifyClientSecret.trim()) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_secret is empty');
      validationError = t('validation.clientSecretRequired');
      return;
    }
    if (spotifyClientSecret.trim().length < 32) {
      console.error('[ONBOARDING] connectSpotify: validation failed - client_secret too short');
      validationError = t('validation.clientSecretTooShort');
      return;
    }

    // #394: in-flight flag set BEFORE the first await blocks double-clicks.
    // Placed after (sync) validation so a failed validation cannot latch
    // the flag and permanently disable the button; no await runs between
    // entry and here, so no interleave window exists.
    if (spotifyConnecting) return;
    spotifyConnecting = true;
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetSpotifyAuthFlow();
    devLog('[ONBOARDING] connectSpotify: ENTRY');
    devLog('[ONBOARDING] connectSpotify: spotifyClientId.length=', spotifyClientId.length);
    devLog('[ONBOARDING] connectSpotify: redirectUri=presencejam://callback');

    setSpotifyPhase('waiting');
    try {
      devLog('[ONBOARDING] connectSpotify: calling invoke start_spotify_auth');
      await invoke('start_spotify_auth', {
        clientId: spotifyClientId,
        clientSecret: spotifyClientSecret,
        redirectUri: 'presencejam://callback'
      });
      devLog('[ONBOARDING] connectSpotify: invoke SUCCESS');
      devLog('[ONBOARDING] connectSpotify: setSpotifyPhase(waiting)');
    } catch (e) {
      console.error('[ONBOARDING] connectSpotify: invoke FAILED:', e);
      setSpotifyPhase('error', e instanceof Error ? e.message : String(e));
      devLog('[ONBOARDING] connectSpotify: setSpotifyPhase(error)');
    } finally {
      spotifyConnecting = false;
    }

    devLog('[ONBOARDING] connectSpotify: EXIT');
  }

  async function handleManualUrlPaste() {
    // #545: in-flight guard set BEFORE the first await, mirroring the other
    // flow entry points (#394). Without it a double-click (or Enter in the
    // URL field) issued two `complete_spotify_auth_manual` calls and the
    // second consumed an already-used authorization code.
    if (manualSubmitBusy) return;
    manualSubmitBusy = true;
    devLog('[ONBOARDING] handleManualUrlPaste: ENTRY');
    devLog('[ONBOARDING] handleManualUrlPaste: spotifyManualUrl.length=', spotifyManualUrl.length);

    try {
      const extracted = extractCodeFromUrl(spotifyManualUrl);
      devLog('[ONBOARDING] handleManualUrlPaste: extracted code:', extracted ? 'present' : 'null');

      if (extracted) {
        // A prior failure must not survive a success: the
        // `{#if validationError}` block sits outside the phase branches, so
        // a stale "No code found in that URL" used to render above the
        // 'Connected to Spotify' badge.
        validationError = '';
        devLog('[ONBOARDING] handleManualUrlPaste: calling invoke complete_spotify_auth_manual');
        // Pass the OAuth `state` through so the backend can validate it
        // against the stored value (CSRF check) — see issue #162.
        await invoke('complete_spotify_auth_manual', {
          code: extracted.code,
          oauthState: extracted.state
        });
        devLog('[ONBOARDING] handleManualUrlPaste: invoke SUCCESS');

        setSpotifyPhase('done');
        devLog('[ONBOARDING] handleManualUrlPaste: setSpotifyPhase(done)');
      } else {
        devLog('[ONBOARDING] handleManualUrlPaste: no code extracted');
        validationError = t('validation.noCodeInUrl');
      }
    } catch (e) {
      console.error('[ONBOARDING] handleManualUrlPaste: FAILED:', e);
      validationError = e instanceof Error ? e.message : String(e);
    } finally {
      manualSubmitBusy = false;
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
    // #394: in-flight flag set BEFORE the first await blocks double-clicks.
    if (teamsConnecting) return;
    teamsConnecting = true;
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetTeamsAuthFlow();
    devLog('[ONBOARDING] connectTeams: ENTRY');
    setTeamsPhase('waiting');

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
        interval: response.interval,
        expiresAt: expiresAtFromResponse(response)
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

      // Auto-poll once the user opens the browser. The user can also retry
      // manually. #785: the poll itself (mutex, expiry guard, phase) lives in
      // the shared store function.
      void pollTeamsAuth();
    } catch (e) {
      console.error('[ONBOARDING] connectTeams: FAILED:', e);
      setTeamsPhase('error', String(e));
    } finally {
      teamsConnecting = false;
    }

    devLog('[ONBOARDING] connectTeams: EXIT');
  }

  async function finish() {
    devLog('[ONBOARDING] finish: ENTRY');
    if (isFinishing) return;
    devLog('[ONBOARDING] finish: spotifyConnected=', spotifyConnected);
    devLog('[ONBOARDING] finish: teamsConnected=', teamsConnected);

    if (!spotifyConnected || !teamsConnected) {
      console.error('[ONBOARDING] finish: validation failed - spotifyConnected=', spotifyConnected, ', teamsConnected=', teamsConnected);
      validationError = t('validation.connectBothFirst');
      return;
    }

    isFinishing = true;
    try {
      devLog('[ONBOARDING] finish: step 1 - reading stored config');
      // #531/#542: MERGE, never replace. The wizard is reachable by
      // returning users (Dashboard goToSetup, Settings "Run onboarding",
      // Reconnect), and the previous implementation built a whole
      // `AppConfig` from this component's defaults — silently resetting
      // their quiet hours, track rules, profanity filter/placeholder,
      // polling bounds, logging level and start_minimized.
      //
      // The read is deliberately `invoke('load_config')` rather than
      // `loadConfig()`: the store helper swallows a failure into
      // `defaultConfig`, which would reintroduce exactly that clobber on a
      // read error. A failed read must abort the save, not invent a base.
      let stored: AppConfig;
      try {
        stored = await invoke<AppConfig>('load_config');
        devLog('[ONBOARDING] finish: stored config read');
      } catch (e) {
        console.error('[ONBOARDING] finish: load_config FAILED, aborting save:', e);
        validationError = t('validation.setupFailed', {
          error: typeof e === 'string' ? e : (e as Error)?.message || String(e)
        });
        return;
      }

      // The Spotify client_secret is sent once to `start_spotify_auth`
      // (which writes it to the OS keychain) and is NOT part of the config
      // saved to disk. See issue #9.
      const cfg: AppConfig = mergeWizardConfig(stored, {
        spotify_client_id: spotifyClientId,
        status_format: statusFormat,
        default_interval_seconds: BigInt(pollingInterval),
        autostart: launchAtLogin
      });
      devLog('[ONBOARDING] finish: config merged');

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
      validationError = t('validation.setupFailed', { error: typeof e === 'string' ? e : (e as Error)?.message || String(e) });
    } finally {
      isFinishing = false;
    }

    devLog('[ONBOARDING] finish: EXIT');
  }
</script>

<div class="onboarding">
  <header class="brand">
    <Logo size={32} withWordmark />
    <div class="brand-right">
      <span class="step-label" role="status">{t('onboarding.stepOf', { step })}</span>
      {#if alreadyComplete}
        <button type="button" class="btn-secondary back-link" onclick={goToDashboard}>
          {t('common.backToDashboard')}
        </button>
      {/if}
    </div>
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
      <div class="card pane-card wizard-card">
        <h2 id="onboarding-step-heading" tabindex="-1">{t('onboarding.step1Title')}</h2>
        <p>
          {t('onboarding.step1Intro')}
        </p>

        <div class="instructions-box">
          <h3>{t('onboarding.getCredentials')}</h3>
          <ol>
            <li>{t('onboarding.instruction1')} (<a href="https://developer.spotify.com/dashboard" target="_blank" rel="noopener">developer.spotify.com</a>)</li>
            <li>{t('onboarding.instruction2')} (<code>presencejam://callback</code>)</li>
            <li>{t('onboarding.instruction3')}</li>
          </ol>
        </div>

        <div class="form-group">
          <label for="client-id">{t('settings.clientId')}</label>
          <input
            id="client-id"
            type="text"
            bind:value={spotifyClientId}
            placeholder={t('onboarding.clientIdPlaceholder')}
            autocomplete="off"
            spellcheck="false"
          />
        </div>
        <div class="form-group">
          <label for="client-secret">{t('settings.clientSecret')}</label>
          <input
            id="client-secret"
            type="password"
            bind:value={spotifyClientSecret}
            placeholder={t('onboarding.clientSecretPlaceholder')}
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
            disabled={!spotifyClientId || !spotifyClientSecret || spotifyConnecting}>
            {t('onboarding.connectSpotify')}
          </button>
        {:else if spotifyWaiting}
          <div class="waiting-box">
            <div class="spinner" aria-hidden="true"></div>
            <p>{t('onboarding.signInWaiting')}</p>
            <p class="hint" id="manual-url-hint">{t('onboarding.manualUrlHint')}</p>
            <label class="sr-only" for="manual-url">{t('onboarding.manualUrlLabel')}</label>
            <input
              id="manual-url"
              type="text"
              bind:value={spotifyManualUrl}
              placeholder={t('onboarding.manualUrlPlaceholder')}
              aria-describedby="manual-url-hint"
              onkeydown={(e) => e.key === 'Enter' && handleManualUrlPaste()}
            />
            <button class="btn-secondary" onclick={handleManualUrlPaste} disabled={manualSubmitBusy}>
              {manualSubmitBusy ? t('onboarding.submitting') : t('onboarding.submitCode')}
            </button>
          </div>
        {:else}
          <div class="success-badge">
            <span aria-hidden="true">✓</span> {t('onboarding.connectedToSpotify')}
          </div>
          <button class="btn-full" onclick={() => { step = 2; devLog('[ONBOARDING] step changed to 2'); focusStepHeading(); }}>{t('onboarding.continue')}</button>
        {/if}
      </div>
    {:else if step === 2}
      <div class="card pane-card wizard-card">
        <h2 id="onboarding-step-heading" tabindex="-1">{t('onboarding.step2Title')}</h2>
        <p>
          {t('onboarding.step2Intro')}
        </p>

        {#if !teamsConnected && !teamsPolling}
          <button class="btn-full" onclick={connectTeams} disabled={teamsConnecting}>{t('onboarding.startMicrosoftSignIn')}</button>
        {:else if teamsPolling}
          <!-- #952: the device-code block is shared with Settings and
               Reconnect, so all three panes render the same box. -->
          <DeviceCodeBox
            userCode={teamsUserCode}
            verificationUrl={teamsVerificationUrl}
            remainingMs={teamsRemainingMs}
            expired={teamsCodeExpired}
            busy={teamsPollMutex.inFlight}
            onCheckNow={() => void pollTeamsAuth()}
            onNewCode={connectTeams}
          />
        {:else}
          <div class="success-badge">
            <span aria-hidden="true">✓</span> {t('onboarding.connectedToTeams')}
          </div>
          <button class="btn-full" onclick={() => { step = 3; devLog('[ONBOARDING] step changed to 3'); focusStepHeading(); }}>{t('onboarding.continue')}</button>
        {/if}

        {#if teamsAuthError}
          <p class="error-message" role="alert">{teamsAuthError}</p>
        {/if}
      </div>
    {:else}
      <div class="card pane-card wizard-card">
        <h2 id="onboarding-step-heading" tabindex="-1">{t('onboarding.step3Title')}</h2>
        <p>
          {t('onboarding.step3Intro')}
        </p>

        <div class="form-group">
          <label for="status-format-onb">{t('onboarding.statusTemplate')}</label>
          <input
            id="status-format-onb"
            type="text"
            bind:value={statusFormat}
            placeholder={t('settings.formatTemplatePlaceholder')}
          />
          <p class="hint">
            {t('onboarding.placeholdersHint')}
          </p>
        </div>

        <div class="form-group">
          <label for="poll-interval-onb">{t('onboarding.pollInterval', { seconds: pollingInterval })}</label>
          <input id="poll-interval-onb" type="range" min="10" max="60" step="5" bind:value={pollingInterval} />
        </div>

        <div class="toggle-row">
          <label for="launch-at-login-onb">{t('common.launchAtLogin')}</label>
          <input id="launch-at-login-onb" type="checkbox" bind:checked={launchAtLogin} />
        </div>

        {#if validationError}
          <p class="error-message" role="alert">{validationError}</p>
        {/if}

        <button class="btn-full" onclick={finish} disabled={isFinishing}>
          {isFinishing ? t('onboarding.settingUp') : t('onboarding.finishSetup')}
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

  /* #967: the step pill and the escape hatch sit together on the right of the
     brand header. `.brand-right .back-link` beats the local
     `.btn-secondary { width: 100% }` below on specificity, so the header
     control stays intrinsic-width. */
  .brand-right {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .brand-right .back-link {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
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
  /* #751: `.card` (background, border, radius, padding), `.hint`,
     `.error-message`, `.sr-only`, `.spinner` and `.btn-full` are global now.
     This pane only says how its cards stack and how wide they breathe. */
  .wizard-card {
    padding: var(--sp-6);
    gap: var(--sp-4);
    box-shadow: var(--shadow-2);
  }
  h2 {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.02em;
  }

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


  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-2) 0;
  }
  .toggle-row label { color: var(--fg); font-size: var(--fs-base); }

  .btn-secondary { width: 100%; }
</style>
