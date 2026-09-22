<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, loadConfig, clientSecretStateOf } from '$lib/stores/config';
  import type { AppConfig, DeviceCodeResponse, SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase, setTeamsDeviceCode, expiresAtFromResponse, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, pollTeamsAuth } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { devLog } from '$lib/utils/dev';
  import DeviceCodeBox from './DeviceCodeBox.svelte';
  import PageHeader from './PageHeader.svelte';
  import { t } from '$lib/i18n';
  import { shouldAutoStartSpotifyReconnect } from '$lib/utils/reconnect';

  let needsSpotify = $state(false);
  let needsTeams = $state(false);

  // #560: the keychain could not be read at all (locked Secret Service, no
  // daemon, denied access), as opposed to `needsSpotify`'s "there really is no
  // stored credential". The two demand opposite advice — unlock the keychain
  // versus re-enter the Client ID/Secret — so they must never be conflated.
  let keychainUnavailable = $state(false);
  // #530: whether Spotify itself has a session to repair. `needsSpotify` only
  // says the credentials are missing, so it cannot answer that on its own —
  // without this, a user whose Teams token alone lapsed got an unsolicited
  // Spotify OAuth window on mount.
  let spotifyConnected = $state(false);

  // #500: in-session Teams completion flag — distinguishes "reconnected
  // just now" (success wording) from "was already connected, no action
  // needed" (neutral wording) on fresh mount.
  let teamsReconnectedThisSession = $state(false);
  // Issue #766: a corrupt-key failure (the stored token encryption key is
  // unusable, or tokens.json fails AES-GCM authentication) surfaces through
  // the Spotify/Teams error channels as English Rust-side copy, so the
  // affordance below keys off stable fragments of those two messages rather
  // than the full text. `tokenResetArmed` is the two-step confirm arm;
  // `tokenResetBusy` serialises the reset invoke; `tokenResetDone` keeps the
  // success copy up after the error phases below are cleared.
  let tokenResetArmed = $state(false);
  let tokenResetBusy = $state(false);
  let tokenResetDone = $state('');
  let tokenResetError = $state('');

  const CORRUPT_KEY_MARKERS = [
    'encryption key is unusable',
    'AES-GCM authentication'
  ];

  function errorLooksLikeCorruptKey(message: string | null): boolean {
    if (!message) return false;
    return CORRUPT_KEY_MARKERS.some((marker) => message.includes(marker));
  }

  let corruptKeyError = $derived(
    errorLooksLikeCorruptKey(authFlow.spotify.error) || errorLooksLikeCorruptKey(authFlow.teams.error)
  );

  function armTokenReset() {
    tokenResetArmed = true;
    tokenResetError = '';
  }

  function cancelTokenReset() {
    tokenResetArmed = false;
  }

  async function confirmTokenReset() {
    if (tokenResetBusy) return;
    tokenResetBusy = true;
    tokenResetError = '';
    try {
      await invoke('reset_local_token_storage');
      devLog('[RECONNECT] token storage reset');
      resetSpotifyAuthFlow();
      resetTeamsAuthFlow();
      tokenResetDone = t('reconnect.resetTokenDone');
      tokenResetArmed = false;
    } catch (e) {
      devLog('[RECONNECT] reset_local_token_storage failed:', e);
      tokenResetError = t('reconnect.resetTokenFailed', { error: String(e) });
    } finally {
      tokenResetBusy = false;
    }
  }

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

  // #615: the teardown is returned synchronously and covers the
  // unmount-while-registering race internally, so this is just a handle.
  let unlisten: (() => Promise<void>) | null = null;
  // #392: onMount awaits config/keychain/sync IPC before it writes state,
  // so an unmount while suspended must abandon the rest of the sequence.
  // (Registration itself is guarded by the helper above.)
  let destroyed = false;

  onMount(async () => {
    devLog('[RECONNECT] onMount: ENTRY');
    if (destroyed) return;
    await loadConfig();
    if (destroyed) return;

    // The client_secret no longer lives in the config — it lives in the OS
    // keychain. We check both client_id (in config) and the keychain
    // presence. See issue #9.
    //
    // #560: `loadConfig` (just awaited above) stamps `client_secret_state`
    // from one keychain probe, so it answers both questions the old
    // `is_spotify_client_secret_set` probe asked — and answers the second one
    // truthfully. That command is a bool, so a locked keyring read as "there is
    // no secret" and sent the user to full setup with their secret still
    // stored.
    const hasClientId = !!$configStore.spotify.client_id
      && $configStore.spotify.client_id.trim() !== '';
    if (destroyed) return;
    const secretState = clientSecretStateOf($configStore);
    const hasClientSecret = secretState === 'present';
    keychainUnavailable = secretState === 'unavailable';
    if (destroyed) return;
    // Only a positively absent credential is a missing credential. An
    // unavailable keychain is not: it gets the "unlock it" banner instead of
    // the setup redirect, and `needsSpotify` stays false so the card does not
    // claim the credentials are gone.
    needsSpotify = !hasClientId || (!hasClientSecret && !keychainUnavailable);
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

    // #615: every suspension point above re-checks `destroyed`, and the
    // helper's own `listen()` calls are the next thing this function does —
    // so no unmount can slip between that check and this registration.
    unlisten = useAuthListeners({
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
        teamsReconnectedThisSession = true;
      },
      onTeamsFailed: (payload) => {
        devLog('[RECONNECT] EVENT: teams-auth-failed:', payload);
        setTeamsPhase('error', String(payload));
      }
    });

    // Auto-start only when Spotify is the provider that needs the sign-in: an
    // unsolicited OAuth window for a healthy session is user-hostile (#530).
    // An unavailable keychain is folded into `credentialsMissing` for the same
    // reason a missing credential is: the flow cannot read the secret it needs,
    // so opening the browser would only produce a failure the user cannot act
    // on. The view explains how to unlock it instead.
    if (
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: needsSpotify || keychainUnavailable,
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
      // Re-check the config (and therefore the keychain) before opening a
      // browser window: the user may have wiped, locked or unlocked it since
      // this view mounted. `loadConfig` is the surface that carries the
      // tri-state, so one call distinguishes "the secret is gone" from "the
      // keychain would not answer" — the bool-only probe collapsed the second
      // into the first and sent the user to setup for a secret that is still
      // stored. See issues #9, #560.
      await loadConfig();
      const secretState = clientSecretStateOf($configStore);
      keychainUnavailable = secretState === 'unavailable';
      if (keychainUnavailable) {
        devLog('[RECONNECT] reconnectSpotify: keychain unavailable, not starting a flow');
        return;
      }
      if (secretState !== 'present') {
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
      await checkTeamsSignIn();
    } catch (e) {
      devLog('[RECONNECT] reconnectTeams failed:', e);
      setTeamsPhase('error', String(e));
    }
  }

  /**
   * #785: the shared poll (mutex, expiry guard, phase transitions) plus this
   * view's own post-success state. The callback runs only when the poll
   * actually succeeded, so a skipped or failed poll cannot claim the user is
   * reconnected.
   */
  async function checkTeamsSignIn() {
    await pollTeamsAuth(() => {
      needsTeams = false;
      teamsReconnectedThisSession = true;
    });
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

    <section class="card pane-card">
      <header class="section-header">
        <h2>{t('settings.sectionSpotify')}</h2>
        <span class="badge"
          class:success={authFlow.spotify.phase === 'done'}
          class:warning={authFlow.spotify.phase === 'waiting'}
          class:error={!!authFlow.spotify.error || needsSpotify || keychainUnavailable}>
          <span class="dot"></span>
          {#if authFlow.spotify.phase === 'done'}{t('common.connected')}
          {:else if needsSpotify}{t('reconnect.missingCredentials')}
          {:else if keychainUnavailable}{t('reconnect.keychainUnavailableBadge')}
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
      {:else if keychainUnavailable}
        <!-- #560: no reconnect button here on purpose — the flow reads the
             secret from the very keychain that cannot answer, so offering it
             would only open a browser window that fails. Replaceable action:
             unlock the keychain, then try again (this view re-probes on every
             entry). -->
        <p class="hint">{t('reconnect.keychainUnavailableHint')}</p>
      {:else}
        <p class="hint">{t('reconnect.clickBelowSpotify')}</p>
        <button class="btn-full" onclick={reconnectSpotify}>{t('settings.reconnectSpotify')}</button>
      {/if}
    </section>

        <section class="card pane-card">
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
          {:else}{t('reconnect.readyToReconnect')}{/if}
        </span>
      </header>

      {#if authFlow.teams.phase === 'done' && teamsReconnectedThisSession}
        <p class="hint">{t('reconnect.teamsOk')}</p>
      {:else if !needsTeams}
        <p class="hint">{t('common.connected')}</p>
      {:else if authFlow.teams.phase === 'waiting'}
        <!-- #952: the same device-code block Settings and the wizard render —
             monospace select-all code, accent URL pill, secondary actions —
             instead of the bare `<strong>` this pane used to print. -->
        <DeviceCodeBox
          userCode={authFlow.teams.userCode}
          verificationUrl={authFlow.teams.verificationUrl}
          remainingMs={teamsRemainingMs}
          expired={teamsCodeExpired}
          busy={teamsPollMutex.inFlight}
          onCheckNow={checkTeamsSignIn}
          onNewCode={reconnectTeams}
        />
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
      <div class="info-box card pane-card">
        <div class="info-icon">⚠</div>
        <div>
          <strong>{t('reconnect.missingCredsTitle')}</strong>
          <p class="hint">{t('reconnect.reenterCredsHint')}</p>
        </div>
        <button class="btn-secondary" onclick={goToOnboarding}>{t('reconnect.goToFullSetup')}</button>
      </div>
    {/if}
    {#if corruptKeyError}
      <!-- Issue #766: corrupt-key path — the stored encryption key is
           unusable or tokens.json fails AES-GCM authentication, so a plain
           reconnect cannot succeed. The reset is two-step (arm, then
           confirm): the confirm names what is deleted and the re-sign-in
           that follows, and after a success the flow phases above are
           cleared so this banner gives way to the done copy. -->
      <div class="info-box card pane-card" role="alert">
        <div class="info-icon">⚠</div>
        <div>
          <strong>{t('reconnect.resetTokenStorage')}</strong>
          <p class="hint">{t('reconnect.tokenStorageUnusable')}</p>
          {#if tokenResetArmed}
            <p class="hint">{t('reconnect.resetTokenConfirm')}</p>
          {/if}
          {#if tokenResetDone}
            <p class="hint">{tokenResetDone}</p>
          {/if}
          {#if tokenResetError}
            <p class="error-message" role="alert">{tokenResetError}</p>
          {/if}
        </div>
        {#if !tokenResetArmed}
          <button class="btn-secondary" onclick={armTokenReset}>{t('reconnect.resetTokenStorage')}</button>
        {:else}
          <button class="btn-secondary" onclick={confirmTokenReset} disabled={tokenResetBusy}>{t('reconnect.resetTokenStorage')}</button>
          <button class="btn-secondary" onclick={cancelTokenReset} disabled={tokenResetBusy}>{t('common.dismiss')}</button>
        {/if}
      </div>
    {:else if tokenResetDone}
      <div class="info-box card pane-card" role="status">
        <div class="info-icon">✓</div>
        <div>
          <p class="hint">{tokenResetDone}</p>
        </div>
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

  /* #751: `.card`, `.section-header`, `.hint`, `.error-message`, `.sr-only`
     and `.btn-full` are global now — this view keeps only what is genuinely
     its own (the info box grid below). */


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

</style>
