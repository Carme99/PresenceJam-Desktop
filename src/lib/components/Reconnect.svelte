<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, loadConfig } from '$lib/stores/config';
  import type { AppConfig, DeviceCodeResponse, SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase, setTeamsDeviceCode, expiresAtFromResponse, formatCountdownMs, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, tryAcquireTeamsPoll, releaseTeamsPoll, isSafeHttpUrl } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';
  import PageHeader from './PageHeader.svelte';
  import { t } from '$lib/i18n';
  import { shouldAutoStartSpotifyReconnect } from '$lib/utils/reconnect';

  let needsSpotify = $state(false);
  let needsTeams = $state(false);
  // #530: whether Spotify itself has a session to repair. `needsSpotify` only
  // says the credentials are missing, so it cannot answer that on its own —
  // without this, a user whose Teams token alone lapsed got an unsolicited
  // Spotify OAuth window on mount.
  let spotifyConnected = $state(false);

  // #500: in-session Teams completion flag — distinguishes "reconnected
  // just now" (success wording) from "was already connected, no action
  // needed" (neutral wording) on fresh mount.
  let teamsReconnectedThisSession = $state(false);

  // #558: manual-paste fallback for the Spotify card. A flow stuck in
  // `waiting` (browser tab abandoned) had no way out at all — these track the
  // paste input, its validation error, and the in-flight state of the submit.
  let spotifyManualUrl = $state('');
  let manualUrlError = $state('');
  let manualSubmitBusy = $state(false);
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

  // #419: combined teardown is async — call sites must await it.
  let unlisten: (() => Promise<void>) | null = null;
  // #392: onMount awaits config/keychain/sync IPC before registering auth
  // listeners, so an unmount while suspended must drop the late
  // subscription (Dashboard.svelte:31-33 pattern).
  let destroyed = false;

  onMount(async () => {
    devLog('[RECONNECT] onMount: ENTRY');
    if (destroyed) return;
    await loadConfig();
    if (destroyed) return;

    // The client_secret no longer lives in the config — it lives in the OS
    // keychain. We check both client_id (in config) and the keychain
    // presence. See issue #9.
    const hasClientId = !!$configStore.spotify.client_id
      && $configStore.spotify.client_id.trim() !== '';
    if (destroyed) return;
    let hasClientSecret = false;
    try { hasClientSecret = await invoke<boolean>('is_spotify_client_secret_set'); } catch { hasClientSecret = false; }
    if (destroyed) return;
    needsSpotify = !hasClientId || !hasClientSecret;
    // Teams re-auth is NOT auto-refreshing in general (device-code
    // refresh failures land the user in a re-auth flow — see #151,
    // #157), so surface the Teams reconnect path honestly.
    // Both flags come from sync status rather than hard-coded true, so a card
    // only shows for the provider that actually needs a sign-in: teams_connected
    // is "tokens present", spotify_connected is "tokens present + client_id set"
    // (commands/sync.rs), and the boot gate clears a session whose refresh token
    // is dead (#530).
    try {
      const status = await invoke<SyncStatus>('get_sync_status');
      if (destroyed) return;
      needsTeams = !status.teams_connected;
      spotifyConnected = status.spotify_connected;
    } catch {
      if (destroyed) return;
      needsTeams = true;
      spotifyConnected = false;
    }

    devLog('[RECONNECT] needsSpotify=', needsSpotify, 'needsTeams=', needsTeams);

    const unlistenFn = await useAuthListeners({
      onSpotifyComplete: () => {
        if (destroyed) return;
        devLog('[RECONNECT] EVENT: spotify-auth-complete received');
        setSpotifyPhase('done');
      },
      onSpotifyFailed: (payload) => {
        if (destroyed) return;
        devLog('[RECONNECT] EVENT: spotify-auth-failed:', payload);
        setSpotifyPhase('error', String(payload));
      },
      onTeamsComplete: () => {
        if (destroyed) return;
        devLog('[RECONNECT] EVENT: teams-auth-complete received');
        setTeamsPhase('done');
        teamsReconnectedThisSession = true;
      },
      onTeamsFailed: (payload) => {
        if (destroyed) return;
        devLog('[RECONNECT] EVENT: teams-auth-failed:', payload);
        setTeamsPhase('error', String(payload));
      }
    });
    if (destroyed) {
      await unlistenFn();
      return;
    }
    unlisten = unlistenFn;

    // Auto-start only when Spotify is the provider that needs the sign-in: an
    // unsolicited OAuth window for a healthy session is user-hostile (#530).
    if (
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: needsSpotify,
        spotifyConnected,
        phase: authFlow.spotify.phase
      })
    ) {
      await reconnectSpotify();
    }
  });

  onDestroy(() => {
    destroyed = true;
    if (unlisten) void unlisten();
  });

  // #394: explicit in-flight flag — the phase check above is only set
  // after the keychain await, so set this BEFORE the first await.
  let spotifyReconnecting = false;
  async function reconnectSpotify() {
    if (spotifyReconnecting) return;
    if (authFlow.spotify.phase === 'waiting' || authFlow.spotify.phase === 'done' || needsSpotify) return;
    spotifyReconnecting = true;
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetSpotifyAuthFlow();
    devLog('[RECONNECT] reconnectSpotify: ENTRY');
    try {
      // Re-check the keychain: the user may have wiped it since the page
      // loaded. If the secret is gone we cannot complete the auth flow
      // without re-onboarding, so bail. See issue #9.
      let hasSecret = false;
      try { hasSecret = await invoke<boolean>('is_spotify_client_secret_set'); } catch { hasSecret = false; }
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
    } finally {
      spotifyReconnecting = false;
    }
  }

  // #558: restart a flow that is stuck in `waiting`. `reconnectSpotify` refuses
  // to restart a waiting flow (and `shouldAutoStartSpotifyReconnect` never does
  // either), so clearing the phase first is what makes the retry reachable.
  async function restartSpotifySignIn() {
    devLog('[RECONNECT] restartSpotifySignIn: ENTRY');
    resetSpotifyAuthFlow();
    await reconnectSpotify();
  }

  // #558: complete the flow from a pasted redirect URL — the same fallback
  // Onboarding offers (#385), for the case where the `presencejam://` deep link
  // never reached the app, or the sign-in finished after the browser tab was
  // reopened. Reuses the onboarding.* dictionary keys: it is the same
  // affordance, so it must not carry a second translation of the same copy.
  async function submitManualUrl() {
    if (manualSubmitBusy) return;
    const extracted = extractCodeFromUrl(spotifyManualUrl);
    if (!extracted) {
      manualUrlError = t('validation.noCodeInUrl');
      return;
    }
    manualSubmitBusy = true;
    manualUrlError = '';
    devLog('[RECONNECT] submitManualUrl: calling invoke complete_spotify_auth_manual');
    try {
      await invoke('complete_spotify_auth_manual', {
        code: extracted.code,
        oauthState: extracted.state
      });
      setSpotifyPhase('done');
    } catch (e) {
      devLog('[RECONNECT] submitManualUrl failed:', e);
      manualUrlError = String(e);
      setSpotifyPhase('error', String(e));
    } finally {
      manualSubmitBusy = false;
    }
  }

  /** Extract `code`/`state` from a pasted Spotify redirect URL. */
  function extractCodeFromUrl(url: string): { code: string; state: string } | null {
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      if (!code) {
        devLog('[RECONNECT] extractCodeFromUrl: no code in URL params');
        return null;
      }
      // A missing `state` still passes (empty string) — the backend rejects it,
      // mirroring the deep-link CSRF check. See issue #162.
      return { code, state: parsed.searchParams.get('state') ?? '' };
    } catch (e) {
      devLog('[RECONNECT] extractCodeFromUrl: URL parse failed:', e);
      return null;
    }
  }

  async function reconnectTeams() {
    if (authFlow.teams.phase === 'waiting') return;
    devLog('[RECONNECT] reconnectTeams: ENTRY');
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetTeamsAuthFlow();
    setTeamsPhase('waiting');
    try {
      const response = await invoke<DeviceCodeResponse>('start_teams_auth_device_code');
      setTeamsDeviceCode({
        userCode: response.user_code,
        verificationUrl: response.verification_url,
        deviceCode: response.device_code,
        interval: response.interval,
        expiresAt: expiresAtFromResponse(response)
      });
      try {
        await invoke('open_external_url', { url: response.verification_url });
      } catch (e) {
        console.warn('[RECONNECT] open_external_url failed (non-fatal):', e);
      }
      await pollTeamsAuth();
    } catch (e) {
      devLog('[RECONNECT] reconnectTeams failed:', e);
      setTeamsPhase('error', String(e));
    }
  }

  async function pollTeamsAuth() {
    if (!authFlow.teams.deviceCode) return;
    // Never poll a dead code — the expired box offers a fresh one (#429).
    if (teamsCodeExpired) {
      devLog('[RECONNECT] pollTeamsAuth: code expired, refusing to poll');
      return;
    }
    // #396: shared poll mutex — only one poll_teams_auth at a time across
    // Onboarding/Settings/Reconnect/+layout.
    if (!tryAcquireTeamsPoll()) {
      devLog('[RECONNECT] pollTeamsAuth: another poll in flight, skipping');
      return;
    }
    setTeamsPhase('waiting');
    try {
      await invoke('poll_teams_auth', {
        deviceCode: authFlow.teams.deviceCode,
        interval: authFlow.teams.interval
      });
      setTeamsPhase('done');
      needsTeams = false;
      teamsReconnectedThisSession = true;
    } catch (e) {
      devLog('[RECONNECT] pollTeamsAuth failed:', e);
      setTeamsPhase('error', String(e));
    } finally {
      releaseTeamsPoll();
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
  <PageHeader title={t('reconnect.title')} onBack={goToDashboard} showThemeToggle={false} />

  <div class="content">
    <p class="description">
      {t('reconnect.description')}
    </p>

    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionSpotify')}</h2>
        <span class="badge"
          class:success={authFlow.spotify.phase === 'done'}
          class:warning={authFlow.spotify.phase === 'waiting'}
          class:error={!!authFlow.spotify.error || needsSpotify}>
          <span class="dot"></span>
          {#if authFlow.spotify.phase === 'done'}{t('common.connected')}
          {:else if needsSpotify}{t('reconnect.missingCredentials')}
          {:else if authFlow.spotify.phase === 'waiting'}{t('common.waiting')}
          {:else if authFlow.spotify.error}{t('reconnect.failed')}
          {:else}{t('reconnect.readyToReconnect')}{/if}
        </span>
      </header>

      {#if authFlow.spotify.phase === 'done'}
        <p class="hint">{t('reconnect.spotifyOk')}</p>
      {:else if needsSpotify}
        <p class="hint">{t('reconnect.spotifyNotConfigured')}</p>
      {:else if authFlow.spotify.phase === 'waiting'}
        <p class="hint">{t('reconnect.completeAuthInOpenedBrowser')}</p>
        <button class="btn-full" onclick={restartSpotifySignIn}>{t('reconnect.restartSignIn')}</button>
        <p class="hint" id="spotify-manual-url-hint">{t('onboarding.manualUrlHint')}</p>
        <label class="sr-only" for="spotify-manual-url">{t('onboarding.manualUrlLabel')}</label>
        <input
          id="spotify-manual-url"
          type="text"
          bind:value={spotifyManualUrl}
          placeholder={t('onboarding.manualUrlPlaceholder')}
          aria-describedby="spotify-manual-url-hint"
          onkeydown={(e) => e.key === 'Enter' && submitManualUrl()}
        />
        <button class="btn-secondary" onclick={submitManualUrl} disabled={manualSubmitBusy}>
          {t('onboarding.submitCode')}
        </button>
        {#if manualUrlError}
          <p class="error-message" role="alert">{manualUrlError}</p>
        {/if}
      {:else if authFlow.spotify.error}
        <p class="error-message" role="alert">{authFlow.spotify.error}</p>
        <button class="btn-full" onclick={reconnectSpotify}>{t('reconnect.tryAgain')}</button>
      {:else}
        <p class="hint">{t('reconnect.clickBelowSpotify')}</p>
        <button class="btn-full" onclick={reconnectSpotify}>{t('settings.reconnectSpotify')}</button>
      {/if}
    </section>

        <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionTeams')}</h2>
        <span class="badge"
          class:success={authFlow.teams.phase === 'done' || !needsTeams}
          class:warning={authFlow.teams.phase === 'waiting'}
          class:error={!!authFlow.teams.error && needsTeams}>
          <span class="dot"></span>
          {#if authFlow.teams.phase === 'done'}{t('common.connected')}
          {:else if !needsTeams}{t('common.connected')}
          {:else if authFlow.teams.phase === 'waiting'}{t('common.waiting')}
          {:else if authFlow.teams.error}{t('reconnect.failed')}
          {:else}{t('reconnect.needsReconnect')}{/if}
        </span>
      </header>

      {#if authFlow.teams.phase === 'done' && teamsReconnectedThisSession}
        <p class="hint">{t('reconnect.teamsOk')}</p>
      {:else if !needsTeams}
        <p class="hint">{t('common.connected')}</p>
      {:else if authFlow.teams.phase === 'waiting'}
        <p class="hint">{t('common.openSignInPage')}</p>
        {#if isSafeHttpUrl(authFlow.teams.verificationUrl)}<a href={authFlow.teams.verificationUrl} target="_blank" rel="noopener">{authFlow.teams.verificationUrl}</a>{:else}<span>{authFlow.teams.verificationUrl}</span>{/if}
        <p class="hint">{t('common.enterCodeWhenAsked')}</p>
        <strong>{authFlow.teams.userCode}</strong>
        {#if teamsCodeExpired}
          <p class="error-message" role="alert">{t('common.codeExpired')}</p>
          <button class="btn-full" onclick={reconnectTeams}>{t('common.getNewCode')}</button>
        {:else}
          {#if teamsRemainingMs != null}
            <p class="hint" aria-live="polite">{t('common.codeExpiresIn', { time: formatCountdownMs(teamsRemainingMs) })}</p>
          {/if}
          <p class="hint">{t('common.waitingForSignIn')}</p>
          <button class="btn-full" onclick={pollTeamsAuth} disabled={teamsPollMutex.inFlight}>{t('common.checkNow')}</button>
        {/if}
        {#if authFlow.teams.error}
          <p class="error-message" role="alert">{authFlow.teams.error}</p>
        {/if}
      {:else if authFlow.teams.error}
        <p class="error-message" role="alert">{authFlow.teams.error}</p>
        <button class="btn-full" onclick={reconnectTeams}>{t('reconnect.tryAgain')}</button>
      {:else}
        <p class="hint">{t('reconnect.clickBelowTeams')}</p>
        <button class="btn-full" onclick={reconnectTeams}>{t('reconnect.reconnectTeams')}</button>
      {/if}
    </section>

    {#if needsSpotify}
      <div class="info-box card">
        <div class="info-icon">⚠</div>
        <div>
          <strong>{t('reconnect.missingCredsTitle')}</strong>
          <p class="hint">{t('reconnect.reenterCredsHint')}</p>
        </div>
        <button class="btn-secondary" onclick={goToOnboarding}>{t('reconnect.goToFullSetup')}</button>
      </div>
    {/if}

    {#if authFlow.spotify.phase === 'done'}
      <button class="btn-full" onclick={goToDashboard}>{t('common.backToDashboard')}</button>
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

  /* Visually hidden label for the manual-paste input (same utility the
     Onboarding wizard defines locally — it is not a global class). */
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
