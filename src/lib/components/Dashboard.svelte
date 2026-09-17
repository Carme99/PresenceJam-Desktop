<script lang="ts">
  import { get } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
  import { currentView } from '$lib/stores/app';
  import { detachedPanes, focusDetached } from '$lib/stores/detach';
  import { configStore, loadConfig, saveConfig, clientSecretStateOf } from '$lib/stores/config';
  import type { ErrorEventPayload, SyncStatus, TrackInfo } from '$lib/types';
  import { devLog } from '$lib/utils/dev';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import { presence, hydrate, setSyncing } from '$lib/stores/presence';
  import { notificationsEnabled } from '$lib/stores/notifications';
  import Logo from './Logo.svelte';
  import { t } from '$lib/i18n';
  import { useListenerTeardown } from '$lib/utils/useAuthListeners';

  /**
   * The gate chip's copy, derived from the reason the always-mounted
   * `+layout.svelte` listener recorded in the presence store (#670): the
   * reason is part of the shared state now, so a remount keeps the specific
   * copy instead of falling back to the generic one.
   */
  let gatedLabel = $derived(gatedReasonLabel($presence.gatedReason));

  /**
   * Map a gate reason to its chip label. Mirrors the Rust `teams.rs`
   * constants — 'quiet-hours', 'track-rule', 'manual-status' and
   * 'out of office' — falling back to the generic busy/meeting copy for a
   * presence sample ('busy', 'in a call', …) and for any unknown reason.
   */
  function gatedReasonLabel(reason: string): string {
    switch (reason) {
      case 'quiet-hours':
        return t('dashboard.presenceGatedQuietHours');
      case 'track-rule':
        return t('dashboard.presenceGatedTrackRule');
      case 'manual-status':
        return t('dashboard.presenceGatedManualStatus');
      case 'out of office':
        return t('dashboard.presenceGatedOutOfOffice');
      default:
        return t('dashboard.presenceGated');
    }
  }

  // #670: the sync mirror lives in the shared presence store — the
  // always-mounted layout owns the `sync-started` / `sync-stopped` listeners,
  // so a state change that lands while another view is mounted is not lost
  // with this component.
  let isToggling = $state(false);
  let isRefreshing = $state(false);
  let spotifyConnected = $state(false);
  let teamsConnected = $state(false);
  let currentTrack = $state<TrackInfo | null>(null);
  // #547/#670: statusPreview / presenceGated live in the module-level presence
  // store (see $lib/stores/presence.ts), written by the always-mounted layout
  // listeners, so a remount shows what the poll loop last reported instead of
  // resetting to "Not configured".
  let availabilityAnnouncement = $state<'listening' | 'cleared' | null>(null);
  let availabilityObserved: boolean | null = null;
  let availabilityTimeout: ReturnType<typeof setTimeout> | null = null;
  const AVAILABILITY_CLEARED_DISMISS_MS = 5000;
  let displayError = $state('');
  // #547/#670: the preview is the last status the poll loop confirmed it
  // posted (or the paused placeholder while a track is paused), so it
  // survives a remount; the fallback copy only applies when nothing has been
  // posted yet this session.
  let statusPreview = $derived(
    $presence.paused
      ? ($presence.pausedStatus ?? t('dashboard.paused'))
      : ($presence.postedStatus ?? (currentTrack ? t('dashboard.statusNotConfigured') : t('dashboard.statusNoTrack')))
  );
  // #670: a pause is not a stop — the card stays up and shows the paused
  // state, either from the poller's pause signal or from the hydrated track's
  // own playback flag.
  let isPaused = $derived($presence.paused || currentTrack?.is_playing === false);

  // ── 4.7.0 — S9 (issue #677): the tray snooze ─────────────────────────────
  //
  // `configStore.snooze_until` is the persisted RFC3339 UTC deadline the tray
  // writes (and the backend owns). "Snoozed" is derived from the INSTANT being
  // in the future, never from the field being present: a deadline that has
  // already passed — in this session, or while the app was closed and the
  // startup clamp has not run yet — must not render a chip.
  let nowMs = $state(Date.now());
  let snoozeUntilMs = $derived.by(() => {
    const raw = $configStore.snooze_until;
    if (!raw) return null;
    const parsed = Date.parse(raw);
    return Number.isFinite(parsed) ? parsed : null;
  });
  let snoozeRemainingMs = $derived(
    snoozeUntilMs === null ? null : Math.max(0, snoozeUntilMs - nowMs)
  );
  let snoozeActive = $derived(snoozeRemainingMs !== null && snoozeRemainingMs > 0);
  let isResuming = $state(false);

  /**
   * `m:ss`, or `h:mm:ss` past an hour. Rounded UP like the tray's countdown, so
   * a freshly set 30-minute snooze reads `30:00` rather than `29:59`.
   */
  function formatRemaining(ms: number): string {
    const total = Math.max(1, Math.ceil(ms / 1000));
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = total % 60;
    const pad = (n: number) => String(n).padStart(2, '0');
    return hours > 0
      ? `${hours}:${pad(minutes)}:${pad(seconds)}`
      : `${minutes}:${pad(seconds)}`;
  }

  let snoozeLabel = $derived(
    snoozeActive && snoozeUntilMs !== null && snoozeRemainingMs !== null
      ? t('dashboard.snoozeChip', {
          remaining: formatRemaining(snoozeRemainingMs),
          time: new Date(snoozeUntilMs).toLocaleTimeString(undefined, {
            hour: '2-digit',
            minute: '2-digit'
          })
        })
      : ''
  );
  let displayErrorTimeout: ReturnType<typeof setTimeout> | null = null;
  // #408: goToSetup re-enable timer must be cleared on destroy so a
  // late callback cannot touch state after unmount.
  let goToSetupTimeout: ReturnType<typeof setTimeout> | null = null;
  // #615: one teardown for every `listen()` below. It is created here,
  // before onMount's first `await`, and the "destroy while onMount is
  // suspended" race (#287) — a registration that settles after unmount —
  // is handled inside it, so the old `destroyed` flag is gone.
  const teardown = useListenerTeardown();
  let lastNotifiedId = '';
  // C8: throttle — at most one track-change notification every 5 s.
  const NOTIFICATION_THROTTLE_MS = 5000;
  let lastNotifiedAt = 0;
  // C8: stable numeric id + group so platforms that support it
  // (id reuse / Apple threadIdentifier) replace the existing
  // track-change notification in place instead of stacking a new one
  // for every track during a session.
  const TRACK_NOTIFICATION_ID = 1001;
  const TRACK_NOTIFICATION_GROUP = 'presencejam-track-change';

  onDestroy(() => {
    void teardown.dispose();
    if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
    if (goToSetupTimeout) clearTimeout(goToSetupTimeout);
    if (availabilityTimeout) clearTimeout(availabilityTimeout);
  });

  // #670: a genuine stop — and only a stop — drops the track card. The flag is
  // written by the always-mounted listener, so a stop that landed while
  // another view was mounted still clears the card here on the next mount.
  let stoppedObserved = false;
  $effect(() => {
    const stopped = $presence.stopped;
    if (stopped === stoppedObserved) return;
    stoppedObserved = stopped;
    if (stopped) currentTrack = null;
  });

  // #551: `availabilityListening` is the shared condition and is applied as
  // "listening"; the "cleared" chip is a one-shot transition, so it only shows
  // when this mount observes the flip — a mount that starts out cleared has
  // nothing to announce.
  $effect(() => {
    const listening = $presence.availabilityListening;
    if (listening === availabilityObserved) return;
    const hadPrevious = availabilityObserved !== null;
    availabilityObserved = listening;
    if (listening || hadPrevious) showAvailability(listening);
  });

  // #670: the tray mirror follows the shared sync state, so every transition —
  // the hydrated mount, a `sync-started`/`sync-stopped` handled by the layout,
  // a sync toggle, a panic or a reconnect — refreshes it exactly once. The
  // store notifies on *any* presence update, so the comparison is explicit:
  // an unrelated status/gate/pause event must not re-invoke the backend.
  let syncingObserved: boolean | null = null;
  $effect(() => {
    const isSyncing = $presence.syncing;
    if (isSyncing === syncingObserved) return;
    syncingObserved = isSyncing;
    devLog(`[DASHBOARD] sync state changed: isSyncing=${isSyncing}`);
    void updateMenuState();
  });

  // S9 (issue #677): tick the countdown once a second, and only while a snooze
  // is live. The effect reads `snoozeActive` but never writes the clock itself,
  // so it starts one interval per snooze episode (and clears it when the
  // countdown reaches zero or the user resumes) instead of re-arming on every
  // tick. `nowMs` is a plain read elsewhere, so each tick re-renders the label.
  $effect(() => {
    if (!snoozeActive) return;
    const id = setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => clearInterval(id);
  });

  onMount(async () => {
    devLog('[DASHBOARD] onMount: ENTRY');

    // S9 (issue #677): the tray writes `snooze_until` straight into the config,
    // with no event and no webview involvement, and this view is destroyed on
    // every view switch (`+page.svelte`). Re-reading the config on mount is
    // therefore what makes a snooze started from the tray show up as a chip —
    // including on the way back from the tray while the window was hidden.
    // `loadConfig` resolves with the frontend defaults on a read failure, which
    // here only means the chip stays hidden until the next mount.
    try {
      await loadConfig();
    } catch (e) {
      console.error('[DASHBOARD] onMount: loadConfig FAILED:', e);
    }
    nowMs = Date.now();

    try {
      devLog('[DASHBOARD] onMount: calling invoke get_sync_status');
      const snapshotRevision = get(presence).revision;
      const status = await invoke<SyncStatus>('get_sync_status');
      devLog('[DASHBOARD] initial sync status:', {
        is_syncing: status.is_syncing,
        spotify_connected: status.spotify_connected,
        teams_connected: status.teams_connected,
        current_track: status.current_track?.title ?? null
      });

      spotifyConnected = status.spotify_connected;
      teamsConnected = status.teams_connected;
      currentTrack = status.current_track;
      hydrate(status, snapshotRevision);
    } catch (e) {
      console.error('[DASHBOARD] onMount: get_sync_status FAILED:', e);
    }

    devLog('[DASHBOARD] onMount: setting up spotify-track-changed listener');
    teardown.add(listen('spotify-track-changed', async (event: any) => {
      devLog('[DASHBOARD] EVENT: spotify-track-changed received');
      devLog('[DASHBOARD] EVENT: track.title=', event.payload.title);
      devLog('[DASHBOARD] EVENT: track.artist=', event.payload.artist);
      currentTrack = event.payload;
      await updateMenuState();
      if ($notificationsEnabled && event.payload?.title) {
        const id = `${event.payload.title}::${event.payload.artist}`;
        if (id === lastNotifiedId) return;
        // C8: timestamp throttle — max one notification per 5 s. A
        // throttled track does NOT claim lastNotifiedId, so once the
        // window elapses the genuinely-current track can still notify.
        const now = Date.now();
        if (now - lastNotifiedAt < NOTIFICATION_THROTTLE_MS) return;
        lastNotifiedId = id;
        lastNotifiedAt = now;
        let granted = false;
        try { granted = await isPermissionGranted(); } catch {}
        if (!granted) { try { granted = (await requestPermission()) === 'granted'; } catch {} }
        if (granted) {
          const body = `${event.payload.artist} — ${event.payload.album ?? ''}`.trim();
          try {
            sendNotification({
              title: event.payload.title,
              body,
              icon: event.payload.album_art_url || undefined,
              // C8: replace-in-place per session where supported.
              id: TRACK_NOTIFICATION_ID,
              group: TRACK_NOTIFICATION_GROUP
            });
          } catch {}
        }
      }
    }));
    // #670: presence state is written by +layout.svelte now (always mounted);
    // this component only consumes the shared store — and hydrates it from
    // `get_sync_status` on mount — so a status, gate or pause that lands while
    // another view is on screen is no longer dropped with this component.

    devLog('[DASHBOARD] onMount: setting up error listener');
    teardown.add(listen<ErrorEventPayload>('error', (event) => {
      const payload = event.payload;
      console.error('[DASHBOARD] EVENT: error received:', payload);
      // Issue #79: only `severity: "error"` (i.e. an error the polling
      // loop did not automatically recover from) pops the red banner.
      // `severity: "warning"` events (e.g. a 401 that triggered token
      // refresh, a 429 that triggered backoff) are logged to the
      // console for the developer but do not alarm-fatigue the user
      // with a banner that disappears during the next successful poll.
      if (payload.severity !== 'error') {
        return;
      }
      const message = typeof payload.message === 'string'
        ? payload.message
        : String(payload);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = message;
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    }));

    // toggle-pause is now handled in +page.svelte (always-mounted) — Dashboard no longer owns it (#230).
    // #670: `sync-started` / `sync-stopped` are owned by +layout.svelte for the
    // same reason and mirror into `$presence.syncing`, so this component no
    // longer registers them.

    devLog('[DASHBOARD] onMount: setting up polling-thread-panicked listener');
    teardown.add(listen('polling-thread-panicked', () => {
      // Rust side resets is_syncing in polling.rs:321, but the JS-side
      // mirror was not being flipped — UI would stay "Syncing" forever
      // after a thread panic. See issue #33.
      devLog('[DASHBOARD] EVENT: polling-thread-panicked received');
      setSyncing(false);
      devLog('[DASHBOARD] EVENT: isSyncing=false (panic recovery)');
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = t('dashboard.syncCrashed');
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    }));

    devLog('[DASHBOARD] onMount: setting up reconnect-required listener');
    teardown.add(listen('reconnect-required', () => {
      // Generic reconnect signal from polling.rs:633 (e.g. when the
      // auth refresh loop has been failing for too long). The
      // provider-specific events are handled elsewhere:
      // spotify-reconnect-required in +layout.svelte (issue #220),
      // teams-reconnect-required in +layout.svelte (issue #157);
      // this is the catch-all that takes the user to the reconnect view.
      devLog('[DASHBOARD] EVENT: reconnect-required received');
      setSyncing(false);
      devLog('[DASHBOARD] EVENT: isSyncing=false (reconnect)');
      currentView.set('reconnect');
    }));
  });

  /**
   * #551: render the availability announcement for the structured state the
   * poll loop just reported. "Listening" is a condition and holds until the
   * next event; "cleared" is a one-shot transition, so it is dismissed on a
   * timer rather than branding the Dashboard for the rest of the session.
   */
  function showAvailability(listening: boolean) {
    if (availabilityTimeout) {
      clearTimeout(availabilityTimeout);
      availabilityTimeout = null;
    }
    if (listening) {
      availabilityAnnouncement = 'listening';
      return;
    }
    availabilityAnnouncement = 'cleared';
    availabilityTimeout = setTimeout(() => {
      availabilityAnnouncement = null;
      availabilityTimeout = null;
    }, AVAILABILITY_CLEARED_DISMISS_MS);
  }

  async function toggleSync() {
    if (isToggling) return;
    devLog('[DASHBOARD] toggleSync: ENTRY');
    devLog('[DASHBOARD] toggleSync: isSyncing=', $presence.syncing);

    isToggling = true;
    try {
      if ($presence.syncing) {
        devLog('[DASHBOARD] toggleSync: calling invoke stop_syncing');
        await invoke('stop_syncing');
        setSyncing(false);
        devLog('[DASHBOARD] toggleSync: isSyncing=false');
      } else {
        devLog('[DASHBOARD] toggleSync: calling invoke start_syncing');
        await invoke('start_syncing');
        setSyncing(true);
        devLog('[DASHBOARD] toggleSync: isSyncing=true');
      }
    } catch (e) {
      console.error('[DASHBOARD] toggleSync failed:', e);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = t('dashboard.syncToggleFailed');
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    } finally {
      isToggling = false;
    }

    devLog('[DASHBOARD] toggleSync: EXIT');
  }

  /**
   * S9 (issue #677): the chip's Resume button.
   *
   * Clearing `snooze_until` is a config write, and the config is shared with the
   * tray, so this goes through the same whole-document save every other config
   * write uses (`saveConfig`), against the shared store as the base — never
   * against a locally held copy, which could resurrect a sibling field the user
   * changed elsewhere in the meantime. The poller re-reads the config on its
   * next iteration, so polling resumes within one sleep.
   *
   * On failure the field stays set (the backend only stores what it wrote), so
   * the chip remains and the user can retry — the honest outcome for a write
   * that did not happen.
   */
  async function resumeSnooze() {
    if (isResuming) return;
    devLog('[DASHBOARD] resumeSnooze: ENTRY');
    isResuming = true;
    try {
      await saveConfig({ ...get(configStore), snooze_until: null });
      nowMs = Date.now();
      devLog('[DASHBOARD] resumeSnooze: snooze cleared');
    } catch (e) {
      console.error('[DASHBOARD] resumeSnooze failed:', e);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = t('dashboard.snoozeResumeFailed');
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    } finally {
      isResuming = false;
    }
  }


  async function refreshStatus() {
    if (isRefreshing || !$presence.syncing) return;
    devLog('[DASHBOARD] refreshStatus: ENTRY');
    devLog('[DASHBOARD] refreshStatus: isSyncing=', $presence.syncing);

    isRefreshing = true;
    try {
      devLog('[DASHBOARD] refreshStatus: calling invoke refresh_status');
      await invoke('refresh_status');
      devLog('[DASHBOARD] refreshStatus: calling invoke get_sync_status');
      const snapshotRevision = get(presence).revision;
      const status = await invoke<SyncStatus>('get_sync_status');
      spotifyConnected = status.spotify_connected;
      teamsConnected = status.teams_connected;
      currentTrack = status.current_track;
      hydrate(status, snapshotRevision);
      await updateMenuState();
    } catch (e) {
      console.error('[DASHBOARD] refreshStatus failed:', e);
      if (displayErrorTimeout) clearTimeout(displayErrorTimeout);
      displayError = t('dashboard.refreshFailed');
      displayErrorTimeout = setTimeout(() => { displayError = ''; displayErrorTimeout = null; }, 5000);
    } finally {
      isRefreshing = false;
    }

    devLog('[DASHBOARD] refreshStatus: EXIT');
  }

  function openSettings() {
    devLog('[DASHBOARD] openSettings: ENTRY');
    // C7: while Settings is popped out, focus the detached window
    // instead of navigating currentView (main window never shows the
    // pane content while it is detached).
    if ($detachedPanes.settings) {
      void focusDetached('settings');
      return;
    }
    currentView.set('settings');
    devLog('[DASHBOARD] openSettings: EXIT');
  }

  function openLogs() {
    devLog('[DASHBOARD] openLogs: ENTRY');
    if ($detachedPanes.logs) {
      void focusDetached('logs');
      return;
    }
    currentView.set('logs');
    devLog('[DASHBOARD] openLogs: EXIT');
  }

  function openDiagnostics() {
    devLog('[DASHBOARD] openDiagnostics: ENTRY');
    currentView.set('diagnostics');
    devLog('[DASHBOARD] openDiagnostics: EXIT');
  }

  function openAbout() {
    currentView.set('about');
  }

  let goToSetupHint = $state('');
  let goToSetupDisabled = $state(false);

  async function goToSetup() {
    devLog('[DASHBOARD] goToSetup: ENTRY');
    try {
      await loadConfig();
      // The client_secret now lives in the OS keychain. We check both the
      // config (client_id) and the keychain (client_secret). See issue #9.
      //
      // #560: the keychain answer is tri-state, and only a *positively absent*
      // secret means there is nothing to reconnect with. The old bool probe
      // also answered `false` for an unavailable keychain (locked Secret
      // Service, no daemon, denied storage access), which sent a user whose
      // secret was still stored into full setup. `loadConfig` above already
      // carries the state, so this costs one fewer IPC round-trip too.
      const hasClientId = !!$configStore.spotify.client_id
        && $configStore.spotify.client_id.trim() !== '';
      const hasClientSecret = clientSecretStateOf($configStore) !== 'absent';
      const hasSpotifyCredentials = hasClientId && hasClientSecret;
      devLog('[DASHBOARD] goToSetup: hasSpotifyCredentials=', hasSpotifyCredentials);

      if (hasSpotifyCredentials) {
        // Credentials exist, go to simplified reconnect flow
        devLog('[DASHBOARD] goToSetup: navigating to reconnect');
        currentView.set('reconnect');
      } else {
        // Missing credentials, need full onboarding
        devLog('[DASHBOARD] goToSetup: navigating to onboarding');
        currentView.set('onboarding');
      }
    } catch (e) {
      console.warn('[DASHBOARD] goToSetup failed:', e);
      goToSetupHint = t('dashboard.credentialCheckFailed');
      goToSetupDisabled = true;
      if (goToSetupTimeout) clearTimeout(goToSetupTimeout);
      goToSetupTimeout = setTimeout(() => { goToSetupHint = ''; goToSetupDisabled = false; goToSetupTimeout = null; }, 4000);
    }
    devLog('[DASHBOARD] goToSetup: EXIT');
  }

  function formatDuration(ms: number): string {
    const secs = Math.floor(ms / 1000);
    const mins = Math.floor(secs / 60);
    const remainingSecs = secs % 60;
    return `${mins}:${remainingSecs.toString().padStart(2, '0')}`;
  }

  // `progress_ms` is null for live/unknown-position streams (Spotify
  // documents it as nullable — see issue #165); treat null as "no known
  // position" rather than position 0.
  let progressPercent = $derived(
    currentTrack && currentTrack.progress_ms != null && currentTrack.duration_ms > 0
      ? (currentTrack.progress_ms / currentTrack.duration_ms) * 100
      : 0
  );

  // Helper to refresh the tray menu. #592: the Rust command takes no
  // arguments — it rebuilds from authoritative backend state — so sending
  // the frontend's isSyncing/currentTrack mirrors would be a claim the
  // backend ignores. Keeping the catch here is load-bearing: the
  // track-change listener calls this un-awaited, so a rejection would
  // otherwise surface as an unhandled promise rejection.
  async function updateMenuState() {
    try {
      await invoke('update_tray_menu_state');
    } catch (e) {
      console.error('[DASHBOARD] updateMenuState failed:', e);
    }
  }
</script>

<div class="dashboard">
  <header>
    <div class="header-left">
      <Logo size={32} title={null} />
      <div class="title">
        <h1>PresenceJam</h1>
        <div class="badges">
          <span class="badge" class:success={spotifyConnected} class:error={!spotifyConnected}>
            <span class="dot"></span>{spotifyConnected ? 'Spotify' : t('dashboard.spotifyOff')}
          </span>
          <span class="badge" class:success={teamsConnected} class:error={!teamsConnected}>
            <span class="dot"></span>{teamsConnected ? 'Teams' : t('dashboard.teamsOff')}
          </span>
          {#if $presence.syncing}
            <span class="badge accent"><span class="dot pulse"></span>{t('dashboard.syncing')}</span>
          {/if}
        </div>
      </div>
    </div>
    <div class="header-right">
      <button class="icon-btn" onclick={toggleTheme} title={t('common.themeToggle')} aria-label={t('common.themeToggle')}>
        {$theme === 'dark' ? '☀' : '☾'}
      </button>
      <button class="icon-btn" class:detached={$detachedPanes.logs}
        onclick={openLogs}
        title={$detachedPanes.logs ? t('dashboard.logsDetachedTitle') : t('dashboard.logsTitle')}
        aria-label={$detachedPanes.logs ? t('dashboard.logsDetachedAria') : t('dashboard.openLogsAria')}>📋</button>
      <button class="icon-btn" onclick={openDiagnostics} title={t('dashboard.diagnostics')} aria-label={t('dashboard.openDiagnosticsAria')}>🩺</button>
      <button class="icon-btn" class:detached={$detachedPanes.settings}
        onclick={openSettings}
        title={$detachedPanes.settings ? t('dashboard.settingsDetachedTitle') : t('dashboard.settings')}
        aria-label={$detachedPanes.settings ? t('dashboard.settingsDetachedAria') : t('dashboard.openSettingsAria')}>⚙</button>
      <button class="icon-btn" onclick={openAbout} title={t('dashboard.about')} aria-label={t('dashboard.aboutAria')}>ⓘ</button>
      <button class="icon-btn primary" class:is-on={$presence.syncing} onclick={toggleSync}
        disabled={isToggling} aria-label={$presence.syncing ? t('dashboard.pauseSync') : t('dashboard.resumeSync')}
        title={$presence.syncing ? t('dashboard.pauseSync') : t('dashboard.resumeSync')}>
        {$presence.syncing ? '⏸' : '▶'}
      </button>
    </div>
  </header>

  {#if displayError}
    <div class="error-banner" role="alert">{displayError}</div>
  {/if}

  <main>
    {#if $presence.gated}
      <div class="presence-chip" role="status">{gatedLabel}</div>
    {/if}
    {#if snoozeActive}
      <!-- S9 (issue #677): the tray snooze, mirrored from the persisted config
           with a live countdown and the one action that ends it. -->
      <div class="snooze-chip" role="status">
        <span>{snoozeLabel}</span>
        <button class="snooze-resume" onclick={resumeSnooze} disabled={isResuming}>
          {isResuming ? t('dashboard.snoozeResuming') : t('dashboard.snoozeResume')}
        </button>
      </div>
    {/if}
    {#if availabilityAnnouncement}
      <div class="availability-chip" role="status">
        {availabilityAnnouncement === 'listening'
          ? t('dashboard.availabilityListening')
          : t('dashboard.availabilityCleared')}
      </div>
    {/if}
    {#if !spotifyConnected || !teamsConnected}
      <div class="setup-card card">
        <div class="setup-icon"><Logo size={56} title={null} /></div>
        <h2>{t('dashboard.setupRequired')}</h2>
        <p>{t('dashboard.setupHint')}</p>
        <div class="setup-actions">
          <button class="btn-full" onclick={goToSetup} disabled={goToSetupDisabled}>{t('dashboard.continueSetup')}</button>
          {#if goToSetupHint}
            <p class="hint" role="status">{goToSetupHint}</p>
          {/if}
        </div>
      </div>
    {:else if currentTrack}
      <div class="track-card card">
        {#if currentTrack.album_art_url}
          <img src={currentTrack.album_art_url} alt="" class="album-art" />
        {:else}
          <div class="album-art placeholder" aria-hidden="true">🎵</div>
        {/if}
        <div class="track-info">
          <div class="track-title">{currentTrack.title}</div>
          <div class="track-artist">{currentTrack.artist}</div>
          <div class="track-album">{currentTrack.album}</div>

          {#if !isPaused}
            <div class="playing-indicator">
              <span class="pulse-dot" aria-hidden="true"></span>
              <span>{t('dashboard.playing')}</span>
            </div>
          {:else}
            <div class="paused-indicator"><span aria-hidden="true">⏸</span> {t('dashboard.paused')}</div>
          {/if}

          <div class="progress-bar" aria-hidden="true">
            <div class="progress-fill" style="width: {progressPercent}%"></div>
          </div>
          <div class="progress-time">
            {#if currentTrack.progress_ms != null}
              {formatDuration(currentTrack.progress_ms)} / {formatDuration(currentTrack.duration_ms)}
            {:else}
              <span class="live-label" aria-label={t('dashboard.liveStreamAria')}>{t('dashboard.live')}</span>
            {/if}
          </div>
          <button
            class="btn-refresh"
            onclick={refreshStatus}
            disabled={!$presence.syncing || isRefreshing}
            aria-label={t('dashboard.refreshAria')}
            aria-busy={isRefreshing}
          >⟳ {isRefreshing ? t('dashboard.refreshing') : t('dashboard.refreshStatus')}</button>
        </div>
      </div>

      <div class="status-preview card">
        <h3>{t('dashboard.yourTeamsStatus')}</h3>
        <p class="status-text" aria-live="polite">{statusPreview}</p>
      </div>
    {:else}
      <div class="not-playing card">
        <div class="not-playing-icon" aria-hidden="true">
          <Logo size={64} title={null} />
        </div>
        <h3>{t('dashboard.nothingPlaying')}</h3>
        <p>{t('dashboard.nothingPlayingHint')}</p>
      </div>
    {/if}
  </main>
</div>

<style>
  .dashboard {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-4);
    padding: var(--sp-4) var(--sp-5);
    border-bottom: 1px solid var(--border);
    background: var(--bg-base);
  }
  .header-left {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-width: 0;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  h1 {
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .header-right {
    display: flex;
    gap: var(--sp-2);
    flex-shrink: 0;
  }
  .icon-btn {
    width: 36px;
    height: 36px;
  }
  /* C7: dot badge marks a nav button whose pane is popped out. */
  .icon-btn.detached {
    position: relative;
  }
  .icon-btn.detached::after {
    content: '';
    position: absolute;
    top: -2px;
    right: -2px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent, #4a9eff);
  }
  .icon-btn.primary {
    color: var(--accent-text);
    border-color: var(--border);
  }
  .icon-btn.primary.is-on {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }

  .error-banner {
    background: var(--danger-soft);
    color: var(--danger);
    padding: var(--sp-3) var(--sp-5);
    text-align: center;
    font-size: var(--fs-sm);
    font-weight: 600;
    border-bottom: 1px solid var(--danger);
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    padding: var(--sp-5);
  }

  /* Presence indicators (issue #3.0-P1/P2): the gate chip while the status
     write is suppressed, and the availability-sync bubble state. */
  .presence-chip,
  .availability-chip {
    align-self: flex-start;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-2) var(--sp-3);
    font-size: var(--fs-sm);
    color: var(--fg);
  }

  /* S9 (issue #677): the tray-snooze chip. Same shell as the presence chips,
     with the "still holding your updates back" accent and a Resume button
     beside the countdown so the way out is one click. */
  .snooze-chip {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--accent, var(--border));
    border-radius: var(--r-md);
    padding: var(--sp-2) var(--sp-3);
    font-size: var(--fs-sm);
    color: var(--fg);
  }
  .snooze-resume {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--fg);
    cursor: pointer;
    font-size: var(--fs-sm);
    padding: var(--sp-1) var(--sp-2);
  }
  .snooze-resume:hover:not(:disabled) {
    background: var(--bg-surface);
  }
  .snooze-resume:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .setup-card {
    text-align: center;
    padding: var(--sp-9) var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
  }
  .setup-icon {
    width: 72px;
    height: 72px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    border-radius: var(--r-lg);
    margin-bottom: var(--sp-2);
  }
  .setup-card h2 { font-size: var(--fs-2xl); }
  .setup-card p {
    color: var(--fg-muted);
    max-width: 36ch;
  }
  .setup-actions {
    width: 100%;
    max-width: 280px;
    margin-top: var(--sp-3);
  }

  .track-card {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: var(--sp-4);
    align-items: flex-start;
  }
  .album-art {
    width: 88px;
    height: 88px;
    border-radius: var(--r-md);
    object-fit: cover;
    background: var(--bg-elevated);
  }
  .album-art.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 32px;
  }
  .track-info {
    min-width: 0;
  }
  .track-title {
    font-size: var(--fs-lg);
    font-weight: 600;
    margin-bottom: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-artist {
    color: var(--fg);
    font-size: var(--fs-sm);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-album {
    color: var(--fg-subtle);
    font-size: var(--fs-xs);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .playing-indicator {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--success);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: var(--sp-3) 0 var(--sp-2);
  }
  .paused-indicator {
    color: var(--fg-subtle);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: var(--sp-3) 0 var(--sp-2);
  }
  .pulse-dot {
    width: 8px; height: 8px;
    background: var(--success);
    border-radius: 50%;
    box-shadow: 0 0 0 0 var(--success);
    animation: pulse 1.5s var(--ease-out) infinite;
  }
  @keyframes pulse {
    0%   { box-shadow: 0 0 0 0 var(--success-soft); }
    70%  { box-shadow: 0 0 0 8px transparent; }
    100% { box-shadow: 0 0 0 0 transparent; }
  }

  /* WCAG 2.3.3: honour prefers-reduced-motion — the "Playing" indicator's
   * pulse is decorative (the label carries the state), so freeze it. */
  @media (prefers-reduced-motion: reduce) {
    .pulse-dot {
      animation: none;
    }
  }
  .progress-bar {
    height: 4px;
    background: var(--bg-elevated);
    border-radius: var(--r-pill);
    overflow: hidden;
    margin: var(--sp-3) 0 var(--sp-1);
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent), var(--accent-hover));
    border-radius: var(--r-pill);
    transition: width var(--dur-slow) linear;
  }
  .progress-time {
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
    font-variant-numeric: tabular-nums;
  }
  .live-label {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--success);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .btn-refresh {
    margin-top: var(--sp-3);
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-1) var(--sp-3);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--accent-text);
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    border-radius: var(--r-md);
    cursor: pointer;
  }
  .btn-refresh:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .btn-refresh:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  @media (prefers-reduced-motion: reduce) {
    .btn-refresh {
      transition: none;
    }
  }

  .status-preview {
    padding: var(--sp-4) var(--sp-5);
  }
  .status-preview h3 {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--fg-subtle);
    margin-bottom: var(--sp-2);
    font-weight: 600;
  }
  .status-text {
    font-size: var(--fs-md);
    color: var(--fg);
    word-break: break-word;
  }

  .not-playing {
    text-align: center;
    padding: var(--sp-9) var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
  }
  .not-playing-icon {
    width: 88px; height: 88px;
    background: var(--bg-elevated);
    border-radius: var(--r-lg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--sp-2);
  }
  .not-playing h3 { font-size: var(--fs-xl); }
</style>
