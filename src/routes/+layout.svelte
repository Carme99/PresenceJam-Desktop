<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  // Side-effect import — installs the module-level subscribe that
  // applies the persisted theme (and keeps it in sync with future
  // changes). Without this, theme only applies when Settings mounts.
  import '$lib/stores/theme';
  // #959: bind the <meta name="color-scheme"> to the live, resolved theme
  // (not the user's preference) so native form controls / scrollbars
  // match what the app actually paints.
  import { appliedTheme } from '$lib/stores/theme';
  import { devLog } from '$lib/utils/dev';
  import UpdatePrompt from '$lib/components/UpdatePrompt.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { currentView } from '$lib/stores/app';
  import { t } from '$lib/i18n';
  import { reconcileDetachedPanes } from '$lib/stores/detach';
  import { clientSecretStateOf, loadConfig } from '$lib/stores/config';
  import { useListenerTeardown } from '$lib/utils/useAuthListeners';
  import {
    markStatusPosted,
    markPresenceGated,
    markPresencePaused,
    markPresenceCleared,
    setAvailabilityListening,
    setPlaybackState,
    setSyncing,
    markAuthPersistWarning
  } from '$lib/stores/presence';
  import {
    migrateLegacyNotificationPreference,
    notifyAuthRequired,
    notifySyncStopped,
    notifyUpdateStaged
  } from '$lib/stores/notifications';

  // C7: this layout is shared by every webview window (the SPA fallback
  // hydrates it for detached Logs/Settings windows too). Reconnect flows,
  // auth navigation, and update checks are owned by the main window —
  // registering them per-window would run device-code/OAuth flows twice
  // when both windows mount. Detached windows only inherit the theme
  // side-effect import above.
  // #498: outside the Tauri runtime (plain browser) `getCurrentWindow()`
  // throws synchronously at component init, blanking the page before the
  // +page boot-failure path can mount. Detect the runtime first and
  // render a static notice instead of Tauri-dependent listeners.
  const isTauriRuntime =
    typeof window !== 'undefined' &&
    typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';
  const isMainWindow = isTauriRuntime ? getCurrentWindow().label === 'main' : false;
  import { authFlow, setTeamsPhase, setTeamsDeviceCode, setSpotifyPhase, expiresAtFromResponse, resetTeamsAuthFlow, pollTeamsAuth, teamsPollMutex } from '$lib/stores/authFlow.svelte';
  import type { DeviceCodeResponse, AppConfig } from '$lib/types';
  devLog(`[LAYOUT] PresenceJam build: ${import.meta.env.VITE_APP_BUILD ?? 'dev build'}`);

  // #711: completion events carry the exact stage request id. Keep a bounded
  // map of request state so a cancel can suppress either a queued completion
  // or a notification already waiting on OS permission. Only terminal,
  // acknowledged entries are evictable. If every slot is unresolved, new
  // requests fail closed until one of those entries drains.
  type UpdateStageNotificationState = {
    completionSeen: boolean;
    notificationPending: boolean;
    cancelled: boolean;
  };

  const MAX_TRACKED_UPDATE_STAGES = 32;
  const updateStageNotifications = new Map<string, UpdateStageNotificationState>();
  let updateNotificationDispatchDestroyed = false;

  function stateForUpdateStage(requestId: string): UpdateStageNotificationState | null {
    const existing = updateStageNotifications.get(requestId);
    if (existing) {
      // Refresh insertion order so the bound evicts the oldest acknowledged
      // state, not the request most recently involved in a race.
      updateStageNotifications.delete(requestId);
      updateStageNotifications.set(requestId, existing);
      return existing;
    }

    if (updateStageNotifications.size >= MAX_TRACKED_UPDATE_STAGES) {
      let acknowledgedId: string | undefined;
      for (const [trackedId, state] of updateStageNotifications) {
        if (state.completionSeen && !state.notificationPending) {
          acknowledgedId = trackedId;
          break;
        }
      }
      if (acknowledgedId === undefined) {
        devLog('[LAYOUT] update-stage tracking saturated; notification backpressured');
        return null;
      }
      updateStageNotifications.delete(acknowledgedId);
    }
    const state: UpdateStageNotificationState = {
      completionSeen: false,
      notificationPending: false,
      cancelled: false
    };
    updateStageNotifications.set(requestId, state);
    return state;
  }
  // A request id is reserved by the UI before its command is sent. This is
  // deliberately separate from completion handling: cancellation must be
  // able to find a tombstone even when all other requests are still pending.
  function reserveUpdateStage(requestId: string): boolean {
    if (!requestId) return false;
    const existing = updateStageNotifications.get(requestId);
    if (existing) {
      updateStageNotifications.delete(requestId);
      updateStageNotifications.set(requestId, existing);
      return true;
    }

    if (updateStageNotifications.size >= MAX_TRACKED_UPDATE_STAGES) {
      let acknowledgedId: string | undefined;
      for (const [trackedId, state] of updateStageNotifications) {
        if (state.completionSeen && !state.notificationPending) {
          acknowledgedId = trackedId;
          break;
        }
      }
      if (acknowledgedId === undefined) {
        devLog('[LAYOUT] update-stage start refused; tracking saturated');
        return false;
      }
      updateStageNotifications.delete(acknowledgedId);
    }

    updateStageNotifications.set(requestId, {
      completionSeen: false,
      notificationPending: false,
      cancelled: false
    });
    return true;
  }

  function setUpdateStageCancellation(requestId: string, cancelled: boolean) {
    if (!requestId || !updateStageNotifications.has(requestId)) return;
    const state = updateStageNotifications.get(requestId)!;
    state.cancelled = cancelled;
    const terminalWithoutPendingSend = cancelled
      ? state.completionSeen && !state.notificationPending
      : !state.completionSeen || !state.notificationPending;
    if (terminalWithoutPendingSend) updateStageNotifications.delete(requestId);
  }

  function handleUpdateStageComplete(version: string, requestId: string) {
    if (!requestId) {
      // Events from a pre-#711 backend have no request id to bind. Preserve
      // their notification compatibility; every new backend event is scoped.
      void notifyUpdateStaged(version, () => updateNotificationDispatchDestroyed);
      return;
    }
    const state = stateForUpdateStage(requestId);
    if (!state) {
      devLog('[LAYOUT] update-stage-complete suppressed by tracking backpressure');
      return;
    }
    if (state.completionSeen) {
      devLog('[LAYOUT] duplicate update-stage-complete ignored');
      return;
    }
    if (state.cancelled) {
      updateStageNotifications.delete(requestId);
      devLog('[LAYOUT] update-stage-complete ignored for cancelled stage');
      return;
    }
    state.completionSeen = true;
    state.notificationPending = true;
    const finish = () => {
      state.notificationPending = false;
      if (state.cancelled && updateStageNotifications.get(requestId) === state) {
        updateStageNotifications.delete(requestId);
      }
    };
    void notifyUpdateStaged(
      version,
      () => updateNotificationDispatchDestroyed || state.cancelled
    ).then(finish, finish);
  }


  function stopUpdateNotificationDispatch() {
    updateNotificationDispatchDestroyed = true;
    for (const [requestId, state] of updateStageNotifications) {
      if (state.notificationPending) {
        state.cancelled = true;
        updateStageNotifications.delete(requestId);
      }
    }
  }

  let playbackError = $state('');
  let playbackErrorTimeout: ReturnType<typeof setTimeout> | null = null;

  function showPlaybackError(msg: string) {
    playbackError = msg;
    if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
    playbackErrorTimeout = setTimeout(() => {
      playbackError = '';
      playbackErrorTimeout = null;
    }, 6000);
  }

  // Always-mounted listeners: teams + spotify reconnect + playback-error.
  // Settings no longer owns spotify-reconnect-required (issue #220) to
  // avoid missed events when the user is on Dashboard.
  onMount(() => {
    updateNotificationDispatchDestroyed = false;
    // #498: never touch Tauri IPC outside the runtime (plain browser).
    if (!isTauriRuntime || !isMainWindow) return;
    // #601: the badge map is empty on every load, but detached windows
    // survive a main-window reload — adopt the real window set before the
    // Dashboard can offer "navigate" for a pane that is already out.
    void reconcileDetachedPanes();
    // #675: the pre-4.7 desktop-notification opt-in was a single localStorage
    // boolean; it becomes `config.notifications.track_change` exactly once,
    // and the config store then feeds the per-class preferences every
    // dispatcher below reads. Fire-and-forget: it must never delay boot.
    void loadConfig()
      .then(migrateLegacyNotificationPreference)
      .catch((e) => console.warn('[LAYOUT] notification preference migration failed:', e));
    let unlistenTeams: (() => void) | null = null;
    let unlistenSpotify: (() => void) | null = null;
    let unlistenPlayback: (() => void) | null = null;
    let destroyed = false;
    // #814: the shared poll mutex lives outside this handler (it is released
    // only when the in-flight `poll_teams_auth` invoke settles — up to 900 s
    // — while this handler only awaits the fast device-code request). So a
    // second poller emit landing while the first code's poll is still in
    // flight used to replace the on-screen code and re-open the browser,
    // then drop the new poll on the mutex: the displayed code was never
    // polled. While the stored flow is still `waiting` on a live code, route
    // back to that code instead of starting a fresh one.
    function teamsCodeLive(): boolean {
      return (
        authFlow.teams.phase === 'waiting' &&
        authFlow.teams.deviceCode !== '' &&
        (authFlow.teams.expiresAt == null || authFlow.teams.expiresAt - Date.now() > 0)
      );
    }
    // #814: the shared poll is a long-blocking invoke held under the store's
    // mutex (up to 900 s), while this handler only awaits the fast
    // device-code request. A second poller emit landing while the first
    // code's poll is still in flight therefore used to replace the on-screen
    // code and re-open the browser, then drop the new poll on the mutex — the
    // displayed code was never polled. `pendingTeamsPoll` parks that newest
    // code (read-only snapshot of the mutex: this handler never acquires or
    // releases it, so the #933 holder count is untouched) and re-drives the
    // shared `pollTeamsAuth` — which re-runs #429's expiry guard itself —
    // once the in-flight poll settles. The slot holds at most one entry: a
    // third emit replaces a still-waiting second ("newest wins").
    let pendingTeamsPoll: string | null = null;

    // The reconnect the user just asked for ("Reconnect Teams") is marked
    // `user_initiated` by Rust; only the poller's dead-session emitters toast.
    listen<{ user_initiated?: boolean }>('teams-reconnect-required', async (event) => {
      devLog('[LAYOUT] teams-reconnect-required received');
      if (event.payload?.user_initiated !== true) void notifyAuthRequired();
      if (teamsCodeLive()) {
        devLog('[LAYOUT] teams-reconnect-required: flow already waiting on a live code, keeping it');
        currentView.set('settings');
        return;
      }
      // #421: fresh entry clears this flow's stale phase only; never the sibling's.
      resetTeamsAuthFlow();
      currentView.set('settings');
      try {
        const response = await invoke<DeviceCodeResponse>('start_teams_auth_device_code');
        setTeamsDeviceCode({
          userCode: response.user_code,
          verificationUrl: response.verification_url,
          deviceCode: response.device_code,
          interval: response.interval,
          // #397: carry expiry through the shared helper so the
          // countdown renders for layout-started flows too.
          expiresAt: expiresAtFromResponse(response)
        });
        setTeamsPhase('waiting');
        try {
          await invoke('open_external_url', { url: response.verification_url });
        } catch (e) {
          console.warn('[LAYOUT] open_external_url failed:', e);
        }
        // #814: snapshot the mutex BEFORE driving the shared poll (it is
        // read-only here: the store's `pollTeamsAuth` owns acquire/release,
        // so the #933 holder count is never touched from this handler). When
        // a first flow's poll still holds it, the shared poll drops this
        // code's request — park the newest code and re-drive it once the
        // holder settles. The re-drive calls the shared poll, which re-runs
        // #429's expiry guard itself: a wait that outlived the code offers a
        // fresh one instead of polling a dead one. Check-now targets the same
        // shared poll, so it starts working again once the mutex frees; the
        const pollHeld = teamsPollMutex.inFlight;
        void pollTeamsAuth();
        if (pollHeld) {
          devLog('[LAYOUT] teams-reconnect-required: poll in flight, parking the new code');
          pendingTeamsPoll = response.device_code;
          const settleWait = setInterval(() => {
            if (destroyed || pendingTeamsPoll !== response.device_code) {
              clearInterval(settleWait);
              return;
            }
            if (teamsPollMutex.inFlight) return;
            pendingTeamsPoll = null;
            clearInterval(settleWait);
            void pollTeamsAuth();
          }, 1000);
        }
      } catch (e) {
        console.error('[LAYOUT] teams-reconnect-required: start_teams_auth_device_code failed:', e);
        setTeamsPhase('error', String(e));
      }
    }).then((u) => {
      if (destroyed) u();
      else unlistenTeams = u;
    });

    listen<string>('spotify-reconnect-required', async () => {
      devLog('[LAYOUT] spotify-reconnect-required received');
      currentView.set('settings');
      try {
        // #560: the config is read first because it carries the keychain
        // tri-state, and the tri-state is the only way to tell "no credential
        // stored" from "the keychain could not answer". The old bool probe
        // collapsed the second into the first, so a locked Secret Service at
        // the moment Spotify asked for a reconnect bounced the user to
        // onboarding over a secret that was still stored. Only a positively
        // absent secret means this flow has nothing to reuse.
        const cfg = await invoke<AppConfig>('load_config');
        const hasSecret = clientSecretStateOf(cfg) !== 'absent';
        if (!hasSecret) {
          console.warn('[LAYOUT] spotify-reconnect-required: no stored client_secret, redirecting to onboarding');
          setSpotifyPhase('idle');
          currentView.set('onboarding');
          return;
        }
        const clientId = cfg.spotify.client_id;
        if (!clientId) {
          console.warn('[LAYOUT] spotify-reconnect-required: client_id empty, redirecting to onboarding');
          setSpotifyPhase('idle');
          currentView.set('onboarding');
          return;
        }
        setSpotifyPhase('waiting');
        await invoke('start_spotify_reconnect', {
          clientId,
          redirectUri: 'presencejam://callback'
        });
      } catch (e) {
        console.error('[LAYOUT] start_spotify_reconnect failed:', e);
        setSpotifyPhase('error', String(e));
      }
    }).then((u) => {
      if (destroyed) u();
      else unlistenSpotify = u;
    });

    listen<string>('playback-error', (event) => {
      const msg = typeof event.payload === 'string' ? event.payload : String(event.payload);
      console.warn('[LAYOUT] playback-error received:', msg);
      showPlaybackError(msg);
    }).then((u) => {
      if (destroyed) u();
      else unlistenPlayback = u;
    });

    // #670 / finding D2: presence and sync lifecycle state is process-wide,
    // but the Dashboard that renders it is destroyed on every view switch.
    // These listeners therefore live in the always-mounted main-window
    // layout and write the shared presence store; Dashboard consumes it (and
    // hydrates from `get_sync_status` on mount) instead of owning listeners
    // of its own. `useListenerTeardown` keeps the "unmounted while listen()
    // was still in flight" race (#287) handled exactly as Dashboard did.
    const presenceTeardown = useListenerTeardown();
    presenceTeardown.add(
      listen<{ status?: string }>('presence-updated', (event) => {
        devLog('[LAYOUT] presence-updated received');
        markStatusPosted(String(event.payload?.status ?? ''));
      })
    );
    presenceTeardown.add(
      listen<{ status?: string }>('presence-paused', (event) => {
        devLog('[LAYOUT] presence-paused received');
        markPresencePaused(String(event.payload?.status ?? ''));
      })
    );
    presenceTeardown.add(
      listen('presence-cleared', () => {
        devLog('[LAYOUT] presence-cleared received');
        markPresenceCleared();
      })
    );
    presenceTeardown.add(
      listen<{ reason?: string }>('presence-gated', (event) => {
        devLog('[LAYOUT] presence-gated received');
        markPresenceGated(String(event.payload?.reason ?? ''));
      })
    );
    presenceTeardown.add(
      listen<{ available?: boolean }>('presence-availability-updated', (event) => {
        devLog('[LAYOUT] presence-availability-updated received');
        setAvailabilityListening(event.payload?.available === true);
      })
    );
    presenceTeardown.add(
      listen<{ is_playing?: boolean }>('playback-state-changed', (event) => {
        devLog('[LAYOUT] playback-state-changed received');
        setPlaybackState(event.payload?.is_playing === true);
      })
    );
    presenceTeardown.add(
      listen('sync-started', () => {
        devLog('[LAYOUT] sync-started received');
        setSyncing(true);
      })
    );
    // #675: Rust marks the two `sync-stopped` emitters apart —
    // `self_terminated: true` is the poller's own exit (polling/state.rs), and
    // `false` the explicit user stop (commands::sync.rs), which must not be
    // reported back as a surprise. An unknown/older payload counts as "not
    // self-terminated", so a user stop can never be mislabelled.
    presenceTeardown.add(
      listen<{ self_terminated?: boolean }>('sync-stopped', (event) => {
        devLog('[LAYOUT] sync-stopped received');
        setSyncing(false);
        if (event.payload?.self_terminated === true) void notifySyncStopped();
      })
    );

    // #670 / finding D10: `teams-auth-persist-warning` fires while the
    // sign-in flows own the screen (Onboarding/Reconnect call
    // `poll_teams_auth`), so a Settings-only listener would drop it. The
    // always-mounted layout records it and Settings renders the banner.
    presenceTeardown.add(
      listen<string>('teams-auth-persist-warning', (event) => {
        devLog('[LAYOUT] teams-auth-persist-warning received');
        markAuthPersistWarning(String(event.payload ?? ''));
      })
    );

    // #675 / #711: completion is request-scoped. The tracker suppresses a
    // queued event and rechecks cancellation after notification permission
    // before the plugin call.
    presenceTeardown.add(
      listen<{ version?: string; request_id?: string }>('update-stage-complete', (event) => {
        devLog('[LAYOUT] update-stage-complete received');
        handleUpdateStageComplete(
          String(event.payload?.version ?? ''),
          String(event.payload?.request_id ?? '')
        );
      })
    );

    // The device-code poll is the shared store function (#785): this copy was
    // the one missing the #429 "never poll a dead code" guard. Its cadence is
    // still Rust-side, and `interval` still comes from the DeviceCodeResponse
    // stored in the authFlow store so the server's requested polling rate is
    // honored — see issue #152.

    return () => {
      destroyed = true;
      stopUpdateNotificationDispatch();
      pendingTeamsPoll = null;
      unlistenTeams?.();
      unlistenSpotify?.();
      unlistenPlayback?.();
      void presenceTeardown.dispose();
      if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
    };
  });

  onDestroy(() => {
    if (playbackErrorTimeout) clearTimeout(playbackErrorTimeout);
    stopUpdateNotificationDispatch();
  });
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href="/icon.svg" />
  <link rel="alternate icon" type="image/png" href="/favicon.png" />
  <meta name="color-scheme" content={$appliedTheme} />
</svelte:head>

<a class="skip-link" href="#main-content">{t('routes.skipToMainContent')}</a>
{#if !isTauriRuntime}
  <!-- #498: static notice outside the Tauri runtime — plain-browser
       visitors get an explanation instead of a blank page + uncaught
       metadata exception. Reuses existing keys, no new copy. -->
  <main id="main-content">
    <p>{t('common.bootFailed')}</p>
    <p>{t('dashboard.setupHint')}</p>
  </main>
{:else}
<slot />
{/if}
{#if playbackError}
  <div class="playback-toast" role="alert">
    <span class="toast-msg">{playbackError}</span>
    <button class="toast-dismiss" onclick={() => { playbackError = ''; if (playbackErrorTimeout) { clearTimeout(playbackErrorTimeout); playbackErrorTimeout = null; } }} aria-label={t('common.dismiss')}>×</button>
  </div>
{/if}
{#if isMainWindow}<UpdatePrompt onStageStart={reserveUpdateStage} onStageCancellation={setUpdateStageCancellation} />{/if}

<style>
  .playback-toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-surface, #1e1e1e);
    color: var(--fg, #eee);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(90vw, 480px);
    box-shadow: 0 4px 16px rgba(0,0,0,0.3);
    z-index: 9999;
  }
  .toast-msg { font-size: 14px; line-height: 1.4; }
  .toast-dismiss {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 18px;
    cursor: pointer;
    padding: 2px 6px;
    opacity: 0.7;
  }
  .toast-dismiss:hover { opacity: 1; }

</style>
