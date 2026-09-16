<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
  import { currentView } from '$lib/stores/app';
  import { emitTo } from '@tauri-apps/api/event';
  // C7 multi-window detach: pop-out/pop-back controls.
  import { popOut, popIn } from '$lib/stores/detach';

  // When rendered in the detached `settings-detached` window, "Back" pops
  // the pane back into the main window (closes this one); the onboarding
  // redirect forwards the navigation to the main window first.
  let { detached = false }: { detached?: boolean } = $props();
  import { configStore, saveConfig, loadConfig, defaultConfig } from '$lib/stores/config';
  import type { AppConfig, SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase, formatCountdownMs, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, tryAcquireTeamsPoll, releaseTeamsPoll, isSafeHttpUrl } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import PageHeader from './PageHeader.svelte';
  import { t, i18n, type Locale } from '$lib/i18n';
  import { theme } from '$lib/stores/theme';
  import { notificationsEnabled, setNotificationsEnabled } from '$lib/stores/notifications';
  import { devLog } from '$lib/utils/dev';

  let localConfig = $state<AppConfig>(structuredClone($configStore));
  let isConnected = $state(false);
  let teamsStatusConnected = $state(false);
  let isSaving = $state(false);
  let saveMessage = $state('');
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;

  // C9: dirty-state detection. Polling fields arrive as BigInt over the
  // IPC boundary, which JSON.stringify rejects, so serialize with a
  // BigInt→number replacer on both sides before comparing.
  function serializeForCompare(cfg: AppConfig): string {
    return JSON.stringify(cfg, (_k, v) => (typeof v === 'bigint' ? Number(v) : v));
  }
  let isDirty = $derived(
    serializeForCompare(localConfig) !== serializeForCompare($configStore)
  );

  // C9: effective polling bounds, mirroring Rust `clamp_polling`
  // (src-tauri/src/config.rs:109): minimum clamps to [5, 30] first, then
  // maximum clamps to [effectiveMinimum, 300]. Consumed twice — the
  // max-interval input's native `min` bound (issue #243) and the clamp
  // hint below it. An entered max below min is silently raised on save;
  // the hint surfaces that effective value immediately.
  let pollingClamp = $derived.by(() => {
    const rawMin = Number(localConfig.polling.minimum_interval_seconds);
    const rawMax = Number(localConfig.polling.max_interval_seconds);
    const effMin = Math.min(30, Math.max(5, rawMin));
    const effMax = Math.min(300, Math.max(effMin, rawMax));
    return { active: rawMin > rawMax, effMin, effMax };
  });

  // C9: per-section "Reset to default" using the shared defaults source.
  function resetPresenceDefaults() {
    localConfig.teams.availability_sync = defaultConfig.teams.availability_sync;
    localConfig.teams.presence_gate = defaultConfig.teams.presence_gate;
  }
  function resetStatusFormatDefaults() {
    localConfig.teams.status_format = defaultConfig.teams.status_format;
    localConfig.teams.profanity_filter = defaultConfig.teams.profanity_filter;
    localConfig.teams.profanity_placeholder = defaultConfig.teams.profanity_placeholder;
  }
  // Issue #432: reset the rules section to its (empty) default. Rules are
  // additive with serde defaults, so a default section is always valid.
  function resetRulesDefaults() {
    localConfig.status_rules = structuredClone(defaultConfig.status_rules);
  }
  // Issue #432: format minutes-since-midnight as HH:MM for time inputs.
  function minutesToTime(m: number): string {
    const h = Math.floor(m / 60) % 24;
    const mm = m % 60;
    return `${String(h).padStart(2, '0')}:${String(mm).padStart(2, '0')}`;
  }
  function timeToMinutes(value: string, fallback: number): number {
    const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
    if (!match) return fallback;
    const h = Math.min(23, Math.max(0, Number(match[1])));
    const mm = Math.min(59, Math.max(0, Number(match[2])));
    return h * 60 + mm;
  }
  function resetPollingDefaults() {
    localConfig.polling = structuredClone(defaultConfig.polling);
  }
  function resetAppearanceDefaults() {
    localConfig.autostart = defaultConfig.autostart;
  }

  // #552: a radiogroup must own `role="radio"`/`aria-checked` children with a
  // roving tabindex and arrow-key navigation. The cards declared
  // `aria-pressed`, which assistive tech ignores inside a radiogroup and
  // which carries no single-selection contract at all.
  let themeDarkButton: HTMLButtonElement | undefined = $state();
  let themeLightButton: HTMLButtonElement | undefined = $state();

  function themeRadioKeydown(e: KeyboardEvent, current: 'dark' | 'light') {
    const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
    const isPrev = e.key === 'ArrowLeft' || e.key === 'ArrowUp';
    if (!isNext && !isPrev) return;
    e.preventDefault();
    const next = current === 'dark' ? 'light' : 'dark';
    theme.set(next);
    // Selection follows focus, and the roving tabindex moves with it.
    (next === 'dark' ? themeDarkButton : themeLightButton)?.focus();
  }

  // 3.1.0 notification opt-in — shared store, default off. #549: this used to
  // be a private `$state` mirrored straight into localStorage, so a toggle in
  // a detached Settings window never reached the already-mounted Dashboard.
  let notificationsMessage = $state('');
  let spotifyAuthWaiting = $derived(authFlow.spotify.phase === 'waiting');
  let teamsAuthWaiting = $derived(authFlow.teams.phase === 'waiting');

  // Device-code expiry countdown (issue #429). Reads the shared store, so
  // a flow started in Onboarding/Reconnect keeps its countdown here. The
  // 1s ticker only runs while a code with known expiry is waiting;
  // $effect cleanup clears the interval on unmount, independent of the
  // onMount/onDestroy listener guard (#392).
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
  // Scopes granted on the stored Spotify access token (decoded backend-side
  // from the JWT payload). The tray playback feature needs
  // `user-modify-playback-state`, which existing users don't have until
  // they re-connect once — the banner below nudges them. Issue #3.0-P3.
  let grantedScopes = $state<string[]>([]);
  let playbackScopeMissing = $derived(
    isConnected && !grantedScopes.includes('user-modify-playback-state')
  );
  // Issue #376: set when the setup-path migration emits the one-time
  // `spotify-secret-conflict` event (legacy plaintext in config.json
  // differs from the keychain entry). The banner below prompts a
  // Spotify reconnect; the plaintext is left untouched until then.
  let spotifySecretConflict = $state(false);

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
  let previewSeq = 0;
  let previewDebounce: ReturnType<typeof setTimeout> | null = null;
  // Issue #342: preview the profanity path with a profane sample so a
  // whitespace-only placeholder demonstrates the effective fallback.
  let previewProfaneSample = $state(false);

  // Live preview of the status format template. We delegate the
  // placeholder substitution to Rust (`preview_status`) so the Svelte
  // preview and the runtime polling loop share one implementation —
  // see issue #74. With the filter on, the preview additionally runs
  // through `filter_status` with the live placeholder (issue #342), so
  // what you see matches what Teams gets. Debounced 300 ms + sequence
  // guard to discard stale responses when the user types quickly.
  $effect(() => {
    const format = localConfig.teams.status_format;
    const filter_enabled = localConfig.teams.profanity_filter;
    const placeholder = localConfig.teams.profanity_placeholder;
    const profane_sample = previewProfaneSample;
    if (previewDebounce) clearTimeout(previewDebounce);
    previewDebounce = setTimeout(async () => {
      const my = ++previewSeq;
      try {
        const v = await invoke<string>('preview_status', { format, filter_enabled, placeholder, profane_sample });
        if (my !== previewSeq) return;
        previewText = v;
      } catch (e) {
        if (my !== previewSeq) return;
        console.warn('[SETTINGS] preview_status failed:', e);
        previewText = t('settings.previewUnavailable');
      }
    }, 300);
  });

  let unlistenFns: UnlistenFn[] = [];
  // #419: combined teardown is async — call sites must await it.
  let unlistenAuth: (() => Promise<void>) | null = null;
  // #392: onMount awaits config/sync/scope IPC before registering auth
  // listeners, so an unmount while suspended must drop the late
  // subscription (Dashboard.svelte:31-33 pattern).
  let authListenersDestroyed = false;
  // Issue #376 conflict listener handle + unmount race flag (issue #392
  // pattern from +layout.svelte: `listen()` resolves async, so an unmount
  // before resolution must immediately release the subscription).
  let unlistenSecretConflict: UnlistenFn | null = null;
  let secretConflictDestroyed = false;

  onMount(async () => {
    if (authListenersDestroyed) return;
    await loadConfig();
    if (authListenersDestroyed) return;
    localConfig = structuredClone($configStore);

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

    // NOTE: `spotify-reconnect-required` is handled by the always-mounted
    // listener in +layout.svelte (issue #220). Removing the Settings-only
    // listener avoids double-handling and missed events when Settings is
    // not mounted (the normal Dashboard case).

    // NOTE: `teams-reconnect-required` is handled by the always-mounted
    // listener in +layout.svelte (issue #157). The polling loop can emit
    // it while Settings is not mounted (the normal Dashboard case), so a
    // Settings-only listener would drop the event. The layout listener
    // sets the authFlow phase, navigates to Settings, and starts the
    // device-code flow; Settings renders the code/URI from the store.

    // Auth completion/failure events via the shared helper.
    const unlisten = await useAuthListeners({
      onSpotifyComplete: () => {
        if (authListenersDestroyed) return;
        devLog('[SETTINGS] spotify-auth-complete received');
        setSpotifyPhase('done');
        isConnected = true;
        // A completed reconnect resolves the #376 secret conflict (the
        // current secret is in the keychain; next launch strips the stale
        // plaintext), so dismiss the banner.
        spotifySecretConflict = false;
        // The new token carries the freshly-granted scope set — refresh so
        // the playback banner disappears. Issue #3.0-P3.
        refreshGrantedScopes();
      },
      onSpotifyFailed: (payload) => {
        if (authListenersDestroyed) return;
        console.error('[SETTINGS] spotify-auth-failed:', payload);
        setSpotifyPhase('error', String(payload));
      },
      onTeamsComplete: () => {
        if (authListenersDestroyed) return;
        devLog('[SETTINGS] teams-auth-complete received');
        setTeamsPhase('done');
        teamsStatusConnected = true;
        // The new token carries the freshly-granted scope set — refresh so
        // the presence banner disappears. Issue #3.0-P1/P2.
        refreshTeamsGrantedScopes();
      },
      onTeamsFailed: (payload) => {
        if (authListenersDestroyed) return;
        console.error('[SETTINGS] teams-auth-failed:', payload);
        setTeamsPhase('error', String(payload));
      }
    });
    if (authListenersDestroyed) {
      await unlisten();
    } else {
      unlistenAuth = unlisten;
    }
    // Issue #376: one-time `spotify-secret-conflict` event from the
    // setup-path migration (config.json holds a legacy plaintext secret
    // that differs from the keychain entry). `useAuthListeners` only
    // covers the four auth events, so subscribe directly; the payload
    // message stays Rust-side English (documented limitation) and is
    // only dev-logged — the banner copy below goes through `t()`.
    listen<{ action: string; message: string }>('spotify-secret-conflict', (event) => {
      devLog('[SETTINGS] spotify-secret-conflict received:', event.payload);
      spotifySecretConflict = true;
    }).then((u) => {
      if (secretConflictDestroyed) u();
      else unlistenSecretConflict = u;
    });
  });

  onDestroy(() => {
    if (saveTimeout) {
      clearTimeout(saveTimeout);
      saveTimeout = null;
    }
    if (previewDebounce) {
      clearTimeout(previewDebounce);
      previewDebounce = null;
    }
    authListenersDestroyed = true;
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    if (unlistenAuth) void unlistenAuth();
    secretConflictDestroyed = true;
    if (unlistenSecretConflict) unlistenSecretConflict();
  });

  async function handleSave() {
    isSaving = true;
    saveMessage = '';
    try {
// `localConfig` is a Svelte 5 `$state` proxy; `structuredClone` in
      // `toSavePayload` rejects proxies with a DataCloneError, aborting the
      // save before IPC (#285). Snapshot to a plain object first.
      // Issue #297: adopt the value the backend actually persisted, so the
      // form shows the clamped numbers rather than the raw input.
      localConfig = await saveConfig($state.snapshot(localConfig));
      saveMessage = t('settings.saved');
      if (saveTimeout) clearTimeout(saveTimeout);
      saveTimeout = setTimeout(() => saveMessage = '', 2000);
    } catch (e) { const msg = String((e as Error)?.message ?? e).slice(0, 180); saveMessage = msg || t('settings.failedToSave'); console.error('[SETTINGS] handleSave failed:', e); }
    finally { isSaving = false; }
  }

  async function openLogs() {
    try {
      await invoke('open_logs_folder');
    } catch (e) {
      console.warn('[SETTINGS] open_logs_folder failed:', e);
    }
  }

  async function reconnectSpotify() {
    if (spotifyAuthWaiting || !localConfig.spotify.client_id) return;
    // #550: `reconnect_spotify` clears the tokens and emits
    // `spotify-reconnect-required`, which the always-mounted main-window
    // listener answers with `currentView.set('settings')`. From a popped-out
    // pane that opens a *second* Settings view beside this one, so hand the
    // navigation back to the main window and close this one instead — the
    // same route goToOnboarding takes.
    if (detached) {
      forwardToMain('settings');
      return;
    }
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetSpotifyAuthFlow();
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
    // #550: same detached-pane guard as reconnectSpotify above — the Teams
    // variant emits `teams-reconnect-required`, handled the same way.
    if (detached) {
      forwardToMain('settings');
      return;
    }
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetTeamsAuthFlow();
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
    // Never poll a dead code — the expired box offers a fresh one (#429).
    if (teamsCodeExpired) {
      console.warn('[SETTINGS] pollTeamsAuth: code expired, refusing to poll');
      return;
    }
    // #396: shared poll mutex — only one poll_teams_auth at a time across
    // Onboarding/Settings/Reconnect/+layout.
    if (!tryAcquireTeamsPoll()) {
      devLog('[SETTINGS] pollTeamsAuth: another poll in flight, skipping');
      return;
    }
    setTeamsPhase('waiting');
    try {
      await invoke('poll_teams_auth', {
        deviceCode: authFlow.teams.deviceCode,
        interval: authFlow.teams.interval
      });
      setTeamsPhase('done');
      teamsStatusConnected = true;
    } catch (e) {
      console.error('[SETTINGS] poll_teams_auth failed:', e);
      setTeamsPhase('error', String(e));
    } finally {
      releaseTeamsPoll();
    }
  }

  // #548: unsaved edits must gate navigation, not merely warn. `localConfig`
  // is the only copy of them — Settings is the sole writer of config.json —
  // so Back and "Run onboarding" park their target here while the banner
  // asks for a choice, instead of silently dropping queued rule edits, a
  // changed status template and changed polling bounds.
  type PendingNav = 'back' | 'onboarding';
  let pendingNav = $state<PendingNav | null>(null);

  function performBack() {
    if (detached) {
      // #403: same catch-and-surface guard as goToOnboarding above.
      popIn('settings').catch((e: unknown) => console.warn('[SETTINGS] popIn failed:', e));
      return;
    }
    currentView.set('dashboard');
  }

  function goBack() {
    if (isDirty && !isSaving) {
      pendingNav = 'back';
      return;
    }
    performBack();
  }

  async function toggleNotifications(e: Event) {
    const target = e.currentTarget as HTMLInputElement;
    if (!target.checked) {
      notificationsMessage = '';
      setNotificationsEnabled(false);
      return;
    }
    // #549: the OS prompt's answer decides the flag. It used to be discarded,
    // so a denied permission left a checked toggle over a localStorage 'true'
    // that the Dashboard honoured — notifications then silently never came.
    let granted = false;
    try {
      granted = (await isPermissionGranted()) || (await requestPermission()) === 'granted';
    } catch (err) {
      console.warn('[SETTINGS] notification permission request failed:', err);
    }
    setNotificationsEnabled(granted);
    if (granted) {
      notificationsMessage = '';
      return;
    }
    // The input is `checked={$notificationsEnabled}` and the store stays
    // false, so reset the DOM property this click already flipped.
    target.checked = false;
    notificationsMessage = t('settings.notificationsDenied');
  }

  // #403: catch-and-surface — WebviewWindow creation/focus can reject
  // (e.g. the window was already closed); never leave a floating promise
  // from the PageHeader action slot.
  function handlePopOut() {
    popOut('settings').catch((e: unknown) => console.warn('[SETTINGS] popOut failed:', e));
  }

  /**
   * #550: main-window navigation from a detached pane. `currentView` is
   * main-window-only, so anything that would move it forwards the target and
   * closes this window (C7 pattern from the onboarding link).
   */
  function forwardToMain(view: 'settings' | 'onboarding') {
    void emitTo('main', 'navigate', view);
    // #403: fire-and-forget close with the same catch-and-surface guard.
    popIn('settings').catch((e: unknown) => console.warn('[SETTINGS] popIn failed:', e));
  }

  function performOnboarding() {
    // Used by the Spotify Client Secret hint when the keychain entry is
    // missing. Re-running Onboarding places a fresh secret in the keychain.
    // See issue #9.
    if (detached) {
      forwardToMain('onboarding');
      return;
    }
    currentView.set('onboarding');
  }

  function goToOnboarding() {
    if (isDirty && !isSaving) {
      pendingNav = 'onboarding';
      return;
    }
    performOnboarding();
  }

  /** #548: run the navigation a confirmed Save / Discard asked for. */
  function leaveSettings(target: PendingNav) {
    if (target === 'onboarding') {
      performOnboarding();
      return;
    }
    performBack();
  }

  async function saveAndLeave() {
    await handleSave();
    // A failed save keeps `isDirty` set and reports the error through
    // `saveMessage`; stay on the form rather than navigating away from it.
    if (isDirty) return;
    const target = pendingNav;
    pendingNav = null;
    if (target) leaveSettings(target);
  }

  function discardAndLeave() {
    const target = pendingNav;
    pendingNav = null;
    if (target) leaveSettings(target);
  }
</script>

<div class="settings">
  <PageHeader title={t('settings.title')} onBack={goBack}
    backLabel={detached ? t('settings.popBackIn') : t('common.back')}
    onAction={detached ? undefined : handlePopOut}
    actionTitle={detached ? '' : t('settings.popOutActionTitle')} />
  {#if isDirty}
    <div class="dirty-banner" role="status">
      <span>{t('settings.unsavedChanges')}</span>
      {#if pendingNav}
        <div class="dirty-actions">
          <button type="button" class="btn-secondary" onclick={saveAndLeave} disabled={isSaving}>
            {isSaving ? t('settings.saving') : t('settings.saveAndLeave')}
          </button>
          <button type="button" class="btn-link" onclick={discardAndLeave}>{t('settings.discardChanges')}</button>
          <button type="button" class="btn-link" onclick={() => (pendingNav = null)}>{t('settings.stayHere')}</button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="sections">
    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionSpotify')}</h2>
        <span class="badge" class:success={isConnected && !spotifyAuthWaiting}
              class:warning={spotifyAuthWaiting}
              class:error={!isConnected && !spotifyAuthWaiting}>
          <span class="dot"></span>
          {#if spotifyAuthWaiting}{t('common.reconnecting')}{:else if isConnected}{t('common.connected')}{:else}{t('common.notConnected')}{/if}
        </span>
      </header>
      <div class="form-group">
        <label for="spotify-client-id">{t('settings.clientId')}</label>
        <input
          id="spotify-client-id"
          type="text"
          bind:value={localConfig.spotify.client_id}
          readonly={isConnected}
          placeholder={t('settings.clientIdPlaceholder')}
        />
      </div>
      <div class="form-group">
        <span class="form-label">{t('settings.clientSecret')}</span>
        <p class="hint">
          {#if localConfig.spotify.client_secret_set}
            {t('settings.secretStoredHint')}
          {:else}
            {t('settings.secretNotConfigured')} <button type="button" class="btn-link" onclick={goToOnboarding}>{t('settings.runOnboarding')}</button> {t('settings.toSetUpSpotify')}
          {/if}
        </p>
      </div>
      <div class="connection-row">
        {#if isConnected && !spotifyAuthWaiting}
          <button class="btn-secondary" onclick={reconnectSpotify} disabled={spotifyAuthWaiting}>{t('settings.reconnectSpotify')}</button>
        {:else if spotifyAuthWaiting}
          <span class="hint">{t('settings.completeAuthInBrowser')}</span>
        {/if}
      </div>
      {#if authFlow.spotify.error}
        <p class="error-message" role="alert">{authFlow.spotify.error}</p>
      {/if}
      {#if playbackScopeMissing}
        <div class="scope-banner">
          <span class="hint">{t('settings.playbackScopeBanner')}</span>
          <button type="button" class="btn-link" onclick={reconnectSpotify} disabled={spotifyAuthWaiting}>{t('common.reconnect')}</button>
        </div>
      {/if}
      {#if spotifySecretConflict}
        <div class="scope-banner">
          <span class="hint">{t('settings.spotifySecretConflict')}</span>
          <button type="button" class="btn-link" onclick={reconnectSpotify} disabled={spotifyAuthWaiting}>{t('common.reconnect')}</button>
        </div>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionTeams')}</h2>
        <span class="badge" class:success={teamsStatusConnected && !teamsAuthWaiting}
              class:warning={teamsAuthWaiting}
              class:error={!teamsStatusConnected && !teamsAuthWaiting}>
          <span class="dot"></span>
          {#if teamsAuthWaiting}{t('common.reconnecting')}{:else if teamsStatusConnected}{t('common.connected')}{:else}{t('common.notConnected')}{/if}
        </span>
      </header>
      <p class="hint">{t('settings.teamsAuthHint')}</p>
      <div class="connection-row">
        {#if teamsStatusConnected && !teamsAuthWaiting}
          <button class="btn-secondary" onclick={reconnectTeams} disabled={teamsAuthWaiting}>{t('reconnect.reconnectTeams')}</button>
        {:else if teamsAuthWaiting}
          <div class="device-code-box">
            <p class="hint">{t('common.openSignInPage')}</p>
            {#if isSafeHttpUrl(authFlow.teams.verificationUrl)}
              <a class="verification-url" href={authFlow.teams.verificationUrl} target="_blank" rel="noopener">{authFlow.teams.verificationUrl}</a>
            {:else}
              <span class="verification-url">{authFlow.teams.verificationUrl}</span>
            {/if}
            <p class="hint">{t('common.enterCodeWhenAsked')}</p>
            <div class="code-display" aria-live="polite">{authFlow.teams.userCode}</div>
            {#if teamsCodeExpired}
              <p class="error-message" role="alert">{t('common.codeExpired')}</p>
              <button class="btn-secondary" onclick={reconnectTeams}>{t('common.getNewCode')}</button>
            {:else}
              {#if teamsRemainingMs != null}
                <p class="hint" aria-live="polite">{t('common.codeExpiresIn', { time: formatCountdownMs(teamsRemainingMs) })}</p>
              {/if}
              <div class="spinner" aria-hidden="true"></div>
              <p>{t('common.waitingForSignIn')}</p>
              <button class="btn-secondary" onclick={pollTeamsAuth} disabled={teamsPollMutex.inFlight}>{t('common.checkNow')}</button>
            {/if}
          </div>
          {#if authFlow.teams.error}
            <p class="error-message" role="alert">{authFlow.teams.error}</p>
          {/if}
        {:else}
          <button class="btn-secondary" onclick={reconnectTeams}>{t('reconnect.reconnectTeams')}</button>
        {/if}
      </div>
      {#if teamsScopesMissing}
        <div class="scope-banner">
          <span class="hint">{t('settings.presenceScopeBanner')}</span>
          <button type="button" class="btn-link" onclick={reconnectTeams} disabled={teamsAuthWaiting}>{t('common.reconnect')}</button>
        </div>
      {/if}
    </section>
    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionPresence')}</h2>
        <button type="button" class="btn-link" onclick={resetPresenceDefaults}>{t('common.resetToDefault')}</button>
      </header>
      <div class="toggle-row">
        <label for="availability-sync">{t('settings.availabilitySyncLabel')}</label>
        <input
          id="availability-sync"
          type="checkbox"
          bind:checked={localConfig.teams.availability_sync}
        />
      </div>
      <p class="hint">
        {t('settings.availabilitySyncHint')}
      </p>
      <div class="toggle-row">
        <label for="presence-gate">{t('settings.presenceGateLabel')}</label>
        <input
          id="presence-gate"
          type="checkbox"
          bind:checked={localConfig.teams.presence_gate}
        />
      </div>
      <p class="hint">
        {t('settings.presenceGateHint')}
      </p>
    </section>
    <section class="card">
      <header class="section-header">
        <h2>{t('rules.sectionTitle')}</h2>
        <button type="button" class="btn-link" onclick={resetRulesDefaults}>{t('common.resetToDefault')}</button>
      </header>
      <p class="hint">{t('rules.sectionHint')}</p>
      {#if localConfig.status_rules == null}
        <p class="hint">{t('rules.noQuietHours')}</p>
      {:else}
      <div class="form-group">
        <span class="form-label">{t('rules.quietHoursLabel')}</span>
        {#if localConfig.status_rules.quiet_hours.length === 0}
          <p class="hint">{t('rules.noQuietHours')}</p>
        {/if}
        {#each localConfig.status_rules.quiet_hours as entry, i}
          <div class="rule-row rule-col" role="group" aria-label={t('rules.quietHoursLabel')}>
            <div class="rule-row">
              <input type="checkbox" bind:checked={entry.enabled} aria-label={t('rules.ruleEnabled')} />
              <input
                type="time"
                value={minutesToTime(entry.start_minutes)}
                onchange={(e) => { entry.start_minutes = timeToMinutes((e.currentTarget as HTMLInputElement).value, entry.start_minutes); }}
                aria-label={t('rules.quietStart')}
              />
              <span aria-hidden="true">–</span>
              <input
                type="time"
                value={minutesToTime(entry.end_minutes)}
                onchange={(e) => { entry.end_minutes = timeToMinutes((e.currentTarget as HTMLInputElement).value, entry.end_minutes); }}
                aria-label={t('rules.quietEnd')}
              />
              <button
                type="button"
                class="btn-link"
                onclick={() => { localConfig.status_rules.quiet_hours.splice(i, 1); }}
              >{t('rules.removeRule')}</button>
            </div>
            <div class="rule-row days-row" role="group" aria-label={t('rules.quietDays')}>
              {#each [1, 2, 3, 4, 5, 6, 7] as day}
                <label class="rule-check day-check">
                  <input
                    type="checkbox"
                    checked={entry.days.includes(day)}
                    onchange={(e) => {
                      const on = (e.currentTarget as HTMLInputElement).checked;
                      entry.days = on
                        ? [...entry.days, day].sort()
                        : entry.days.filter((d) => d !== day);
                    }}
                  />
                  <span>{t(`rules.day${day}` as 'rules.day1')}</span>
                </label>
              {/each}
            </div>
          </div>
        {/each}
        <button
          type="button"
          class="btn-secondary"
          onclick={() => { localConfig.status_rules.quiet_hours.push({ enabled: true, start_minutes: 1320, end_minutes: 420, days: [] }); }}
        >{t('rules.addQuietHours')}</button>
      </div>
      <div class="form-group">
        <span class="form-label">{t('rules.trackRulesLabel')}</span>
        {#if localConfig.status_rules.track_rules.length === 0}
          <p class="hint">{t('rules.noTrackRules')}</p>
        {/if}
        {#each localConfig.status_rules.track_rules as rule, j}
          <div class="rule-row rule-col" role="group" aria-label={t('rules.trackRulesLabel')}>
            <label class="rule-check">
              <input type="checkbox" bind:checked={rule.enabled} />
              <span>{t('rules.ruleEnabled')}</span>
            </label>
            <input
              type="text"
              bind:value={rule.artist_substring}
              placeholder={t('rules.artistPlaceholder')}
              aria-label={t('rules.artistPlaceholder')}
            />
            <input
              type="text"
              bind:value={rule.track_substring}
              placeholder={t('rules.trackPlaceholder')}
              aria-label={t('rules.trackPlaceholder')}
            />
            <input
              type="text"
              bind:value={rule.replacement_status}
              placeholder={t('rules.replacementPlaceholder')}
              aria-label={t('rules.replacementPlaceholder')}
            />
            <button
              type="button"
              class="btn-link"
              onclick={() => { localConfig.status_rules.track_rules.splice(j, 1); }}
            >{t('rules.removeRule')}</button>
          </div>
        {/each}
        <button
          type="button"
          class="btn-secondary"
          onclick={() => { localConfig.status_rules.track_rules.push({ enabled: false, artist_substring: '', track_substring: '', replacement_status: '' }); }}
        >{t('rules.addTrackRule')}</button>
      </div>
      {/if}
    </section>
    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionStatusFormat')}</h2>
        <button type="button" class="btn-link" onclick={resetStatusFormatDefaults}>{t('common.resetToDefault')}</button>
      </header>
      <div class="form-group">
        <label for="status-format">{t('settings.formatTemplate')}</label>
        <input
          id="status-format"
          type="text"
          bind:value={localConfig.teams.status_format}
          placeholder={t('settings.formatTemplatePlaceholder')}
        />
      </div>
      <div class="form-group">
        <span class="form-label">{t('settings.livePreview')}</span>
        <div class="preview-box" aria-live="polite">{previewText}</div>
      </div>
      <p class="hint">
        {t('settings.placeholdersHint')}
      </p>
      <div class="toggle-row">
        <label for="profanity-filter">{t('settings.profanityFilterLabel')}</label>
        <input
          id="profanity-filter"
          type="checkbox"
          bind:checked={localConfig.teams.profanity_filter}
        />
      </div>
      {#if localConfig.teams.profanity_filter}
        <div class="form-group">
          <label for="profanity-placeholder">{t('settings.placeholderTextLabel')}</label>
          <p class="hint">
            {t('settings.placeholderTextHint')}
          </p>
          <input
            id="profanity-placeholder"
            type="text"
            bind:value={localConfig.teams.profanity_placeholder}
            placeholder={t('settings.placeholderTextPlaceholder')}
          />
        </div>
        <div class="toggle-row">
          <label for="profanity-preview-sample">{t('settings.profaneSampleToggle')}</label>
          <input
            id="profanity-preview-sample"
            type="checkbox"
            bind:checked={previewProfaneSample}
          />
        </div>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionPolling')}</h2>
        <button type="button" class="btn-link" onclick={resetPollingDefaults}>{t('common.resetToDefault')}</button>
      </header>
      <div class="form-group">
        <label for="default-interval">{t('settings.defaultIntervalLabel', { seconds: Number(localConfig.polling.default_interval_seconds) })}</label>
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
          <label for="min-interval">{t('settings.minIntervalLabel')}</label>
          <input
            id="min-interval"
            type="number"
            min="5"
            max="30"
            bind:value={localConfig.polling.minimum_interval_seconds}
          />
        </div>
        <div class="form-group">
          <label for="max-interval">{t('settings.maxIntervalLabel')}</label>
          <!-- `min` tracks clamp_polling's effective minimum; `max` is the
               backend's fixed upper bound (config.rs) — `pollingClamp.effMax`
               depends on the entered max, so using it here would be
               self-referential. -->
          <input
            id="max-interval"
            type="number"
            min={pollingClamp.effMin}
            max="300"
            bind:value={localConfig.polling.max_interval_seconds}
          />
        </div>
      </div>
      {#if pollingClamp.active}
        <p class="clamp-hint" role="status">
          {t('settings.clampHint', { max: pollingClamp.effMax })}
        </p>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionNotifications')}</h2>
      </header>
      <div class="toggle-row">
        <label for="notifications-enabled">{t('settings.notificationsToggle')}</label>
        <input id="notifications-enabled" type="checkbox" checked={$notificationsEnabled} onchange={toggleNotifications} />
      </div>
      <p class="hint">{t('settings.notificationsHint')}</p>
      {#if notificationsMessage}
        <p class="error-message" role="alert">{notificationsMessage}</p>
      {/if}
    </section>

    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionAppearance')}</h2>
        <button type="button" class="btn-link" onclick={resetAppearanceDefaults}>{t('common.resetToDefault')}</button>
      </header>
      <div class="form-group">
        <span class="form-label">{t('settings.themeLabel')}</span>
        <div class="theme-grid" role="radiogroup" aria-label={t('settings.themeLabel')}>
          <button type="button" class="theme-card" role="radio" bind:this={themeDarkButton}
            aria-checked={$theme === 'dark'} tabindex={$theme === 'dark' ? 0 : -1}
            class:is-active={$theme === 'dark'}
            onclick={() => theme.set('dark')}
            onkeydown={(e) => themeRadioKeydown(e, 'dark')}>
            <span class="swatch swatch-dark"></span>
            <span class="theme-name">{t('settings.themeDark')}</span>
          </button>
          <button type="button" class="theme-card" role="radio" bind:this={themeLightButton}
            aria-checked={$theme === 'light'} tabindex={$theme === 'light' ? 0 : -1}
            class:is-active={$theme === 'light'}
            onclick={() => theme.set('light')}
            onkeydown={(e) => themeRadioKeydown(e, 'light')}>
            <span class="swatch swatch-light"></span>
            <span class="theme-name">{t('settings.themeLight')}</span>
          </button>
        </div>
      </div>
      <div class="form-group">
        <label for="language">{t('settings.languageLabel')}</label>
        <!-- Language names are endonyms: shown in their own language by convention. -->
        <select
          id="language"
          value={i18n.locale}
          onchange={(e) => i18n.set((e.currentTarget as HTMLSelectElement).value as Locale)}
        >
          <option value="en">English</option>
          <option value="de">Deutsch</option>
          <option value="fr">Français</option>
        </select>
      </div>
      <div class="toggle-row">
        <label for="autostart">{t('common.launchAtLogin')}</label>
        <input
          id="autostart"
          type="checkbox"
          checked={localConfig.autostart}
          onchange={async (e) => {
            const target = e.currentTarget as HTMLInputElement;
            const enabled = target.checked;
            const previous = !enabled;
            localConfig.autostart = enabled;
            try {
              await invoke('set_autostart_enabled', { enabled });
            } catch (err) {
              console.warn('[SETTINGS] set_autostart_enabled failed:', err);
              localConfig.autostart = previous;
              target.checked = previous;
              saveMessage = t('settings.autostartError', { error: String(err).slice(0, 120) });
              if (saveTimeout) clearTimeout(saveTimeout);
              saveTimeout = setTimeout(() => saveMessage = '', 3000);
            }
          }}
        />
      </div>
    </section>

    <section class="actions">
      <button class="btn-full" onclick={handleSave} disabled={isSaving}>
        {isSaving ? t('settings.saving') : t('settings.saveChanges')}
      </button>
      {#if saveMessage}
        <p class="save-message" aria-live="polite">{saveMessage}</p>
      {/if}
      <button class="btn-secondary btn-full" onclick={openLogs}>{t('settings.openLogsFolder')}</button>
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

  /* C9: unsaved-changes banner shown when localConfig drifts from the
     saved store; and the polling min>max clamp feedback hint. */
  .dirty-banner {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-4);
    background: var(--warning-soft);
    color: var(--warning);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    font-size: var(--fs-sm);
    font-weight: 600;
    text-align: center;
  }
  /* #548: Save / Discard / Stay, revealed when a navigation is blocked. */
  .dirty-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: var(--sp-3);
  }
  .clamp-hint {
    margin: 0;
    padding: var(--sp-2) var(--sp-3);
    background: var(--warning-soft);
    color: var(--warning);
    border-radius: var(--r-md);
    font-size: var(--fs-xs);
    line-height: var(--lh-normal);
  }

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
  /* Issue #432: status-rule rows reuse the card's form rhythm — a
  wrapping flex row for quiet-hours entries, column variant for the
  four-field track rules. */
  .rule-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .rule-row input[type='text'],
  .rule-row input[type='time'] {
    flex: 1 1 120px;
    min-width: 0;
  }
  .rule-col {
    flex-direction: column;
    align-items: stretch;
  }
  .rule-check {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
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
