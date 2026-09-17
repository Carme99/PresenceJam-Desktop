<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { emitTo } from '@tauri-apps/api/event';
  // C7 multi-window detach: pop-out/pop-back controls.
  import { popOut, popIn } from '$lib/stores/detach';

  // When rendered in the detached `settings-detached` window, "Back" pops
  // the pane back into the main window (closes this one); the onboarding
  // redirect forwards the navigation to the main window first.
  let { detached = false }: { detached?: boolean } = $props();
  import { configStore, saveConfig, loadConfig, defaultConfig, clientSecretStateOf } from '$lib/stores/config';
  import type { AppConfig, SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase, formatCountdownMs, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, tryAcquireTeamsPoll, releaseTeamsPoll, isSafeHttpUrl } from '$lib/stores/authFlow.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import PageHeader from './PageHeader.svelte';
  import { t, i18n, type Locale, type TKey } from '$lib/i18n';
  import { theme } from '$lib/stores/theme';
  import {
    NOTIFICATION_CLASSES,
    notificationPreferences,
    setNotificationPreference,
    type NotificationClass
  } from '$lib/stores/notifications';
  import { presence, clearAuthPersistWarning } from '$lib/stores/presence';
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
    // Findings #635/#637: both new presence policies reset with the card.
    localConfig.teams.respect_manual_status = defaultConfig.teams.respect_manual_status;
    localConfig.teams.gate_when_out_of_office = defaultConfig.teams.gate_when_out_of_office;
  }
  function resetStatusFormatDefaults() {
    localConfig.teams.status_format = defaultConfig.teams.status_format;
    localConfig.teams.profanity_filter = defaultConfig.teams.profanity_filter;
    localConfig.teams.profanity_placeholder = defaultConfig.teams.profanity_placeholder;
    // Issue #538: the custom lexicon belongs to this card too.
    localConfig.teams.profanity_extra_words = [...defaultConfig.teams.profanity_extra_words];
    extraWordsText = '';
  }
  // Issue #432: reset the rules section to its (empty) default. Rules are
  // additive with serde defaults, so a default section is always valid.
  function resetRulesDefaults() {
    localConfig.status_rules = structuredClone(defaultConfig.status_rules);
    // S4 (issue #672): the card also renders the two manual-status texts, so
    // Reset must not leave those editors showing a stale value.
    localConfig.teams.paused_status_format = defaultConfig.teams.paused_status_format;
    localConfig.teams.stopped_status_format = defaultConfig.teams.stopped_status_format;
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

  // S4 (issue #672): a track rule's window END spans 0..=1440, where 1440 is
  // the end of the day (the config default). `<input type="time">` can only
  // express 00:00–23:59, and a picked 00:00 is midnight — the same instant as
  // 1440 — so it is stored as 1440 instead of 0, which would be an empty
  // window that never matches.
  function endMinutesFromTime(value: string, fallback: number): number {
    const minutes = timeToMinutes(value, fallback % 1440);
    return minutes === 0 ? 1440 : minutes;
  }
  // S4 (issue #672): array order is priority (the first matching rule wins), so
  // the card needs a way to reorder the rules.
  function moveRule(
    rules: AppConfig['status_rules']['track_rules'],
    index: number,
    delta: number
  ): void {
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    const [moved] = rules.splice(index, 1);
    rules.splice(target, 0, moved);
  }
  function resetPollingDefaults() {
    localConfig.polling = structuredClone(defaultConfig.polling);
  }

  // ── 4.6 findings #634/#635/#637 + issue #538 consumption sites ──────────
  //
  // Frontend mirrors of the Rust clamps, so a typed/pasted value shows the
  // value the backend will actually store (`config.rs::clamp_rules`,
  // `::clamp_teams`, `::clamp_polling`) instead of silently differing.
  const MAX_RULE_STATUS_CHARS = 128;
  const EXTRA_WORDS_MAX_ENTRIES = 64;
  const EXTRA_WORDS_MAX_CHARS = 32;
  const PAUSE_BACKOFF_MIN_SECONDS = 60;
  const PAUSE_BACKOFF_MAX_SECONDS = 3600;

  /**
   * The five availability/activity pairs Graph `presence: setPresence`
   * accepts. Mirrors `config.rs::PRESENCE_COMBINATIONS` — the closed set the
   * backend normalizes against — and deliberately omits the two the docs say
   * have no effect.
   */
  const PRESENCE_OPTIONS = [
    { availability: 'Available', activity: 'Available', label: 'Available' },
    { availability: 'Busy', activity: 'InACall', label: 'Busy — In a call' },
    {
      availability: 'Busy',
      activity: 'InAConferenceCall',
      label: 'Busy — In a conference call'
    },
    { availability: 'Away', activity: 'Away', label: 'Away' },
    { availability: 'DoNotDisturb', activity: 'Presenting', label: 'Do not disturb — Presenting' }
  ] as const;

  type PresenceFields = { presence_availability: string; presence_activity: string };

  /** `"Availability|Activity"` for the row's `<select>`, `''` when unset. */
  function presenceValue(availability: string, activity: string): string {
    return availability && activity ? `${availability}|${activity}` : '';
  }

  /** Write a selected pair back, or clear BOTH fields for "don't change". */
  function applyPresenceValue(target: PresenceFields, value: string) {
    const [availability, activity] = value.split('|');
    target.presence_availability = availability ?? '';
    target.presence_activity = activity ?? '';
  }

  /**
   * Rust bounds the lexicon to 64 entries of 32 chars at the IPC boundary
   * (issue #538). Counted here so the hint can say what will actually be
   * matched, and applied on save so the store shows what the backend stores.
   */
  let extraWordsText = $state('');
  let extraWordsClamp = $derived.by(() => {
    const raw = extraWordsText
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    const kept = raw.map((word) => [...word].slice(0, EXTRA_WORDS_MAX_CHARS).join(''));
    const truncated = kept.slice(0, EXTRA_WORDS_MAX_ENTRIES);
    return {
      active: raw.length > EXTRA_WORDS_MAX_ENTRIES || kept.some((word, i) => word !== raw[i]),
      kept: truncated.length,
      maxEntries: EXTRA_WORDS_MAX_ENTRIES,
      maxChars: EXTRA_WORDS_MAX_CHARS,
      clamped: truncated
    };
  });

  /** The pause-backoff ceiling a typed value lands on after `clamp_polling`. */
  let pauseBackoffClamp = $derived.by(() => {
    const raw = Number(localConfig.polling.pause_backoff_max_seconds);
    const effective = Math.min(
      PAUSE_BACKOFF_MAX_SECONDS,
      Math.max(PAUSE_BACKOFF_MIN_SECONDS, Number.isFinite(raw) ? raw : 300)
    );
    return { active: effective !== raw, effective };
  });
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

  // #675: one toggle per desktop-notification class. The store is the shared
  // state (persisted to `config.json` through `saveConfig`), so a toggle here
  // reaches the always-mounted main window. #549 still holds: the OS prompt's
  // answer decides whether a class may notify, and a denied permission must
  // not leave a checked toggle behind.
  let notificationsMessage = $state('');
  // Keys are `TKey`, so a class added on the Rust side cannot be rendered
  // with a missing dictionary entry.
  const NOTIFICATION_LABELS: Record<NotificationClass, TKey> = {
    track_change: 'settings.notificationsTrackChange',
    sync_stopped: 'settings.notificationsSyncStopped',
    auth_required: 'settings.notificationsAuthRequired',
    update_staged: 'settings.notificationsUpdateStaged'
  };
  let spotifyAuthWaiting = $derived(authFlow.spotify.phase === 'waiting');
  let teamsAuthWaiting = $derived(authFlow.teams.phase === 'waiting');

  // Device-code expiry countdown (issue #429). Reads the shared store, so
  // a flow started in Onboarding/Reconnect keeps its countdown here. The
  // 1s ticker only runs while a code with known expiry is waiting;
  // $effect cleanup clears the interval on unmount, independent of the
  // onMount/onDestroy listener teardown (#615).
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

  // #693: a Teams sign-in whose tokens could not be persisted (locked
  // keychain, full/read-only disk) must not be lost silently. The event is
  // captured in the always-mounted `routes/+layout.svelte` listener — every
  // sign-in path goes through `poll_teams_auth`, including the ones that do
  // not have Settings mounted — and the shared `presence` store is what this
  // pane renders from. Settings only displays the fault and offers the retry.

  // #560: the OS keychain's answer about the stored client_secret —
  // `present` / `absent` / `unavailable`. The credential row must branch on
  // this rather than on `client_secret_set`, which cannot tell "the user never
  // configured a secret" from "the keychain would not answer".
  let spotifySecretState = $derived(clientSecretStateOf(localConfig));

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
    // Issue #538: the preview must run the user's own lexicon too, otherwise a
    // word they just added shows no effect until the next real track.
    const extra_words = extraWordsClamp.clamped;
    if (previewDebounce) clearTimeout(previewDebounce);
    previewDebounce = setTimeout(async () => {
      const my = ++previewSeq;
      try {
        const v = await invoke<string>('preview_status', { format, filter_enabled, placeholder, profane_sample, extra_words });
        if (my !== previewSeq) return;
        previewText = v;
      } catch (e) {
        if (my !== previewSeq) return;
        console.warn('[SETTINGS] preview_status failed:', e);
        previewText = t('settings.previewUnavailable');
      }
    }, 300);
  });

  // #615: `useAuthListeners` returns one teardown synchronously (it covers
  // both the four auth events and the #376 extra listener below) and tracks
  // the unmount-while-registering race internally, so this is just a handle.
  let teardownAuth: (() => Promise<void>) | null = null;

  onMount(async () => {
    // #615: registered first, synchronously — before the config/scope IPC
    // below can suspend — so the `onDestroy` teardown always has a handle to
    // release. That ordering is what removes the old `destroyed` flags.
    //
    // Issue #376: the one-time `spotify-secret-conflict` event comes from the
    // setup-path migration (config.json holds a legacy plaintext secret that
    // differs from the keychain entry). The payload message stays Rust-side
    // English (documented limitation) and is only dev-logged — the banner
    // copy below goes through `t()`.
    teardownAuth = useAuthListeners(
      {
        onSpotifyComplete: () => {
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
          console.error('[SETTINGS] spotify-auth-failed:', payload);
          setSpotifyPhase('error', String(payload));
        },
        onTeamsComplete: () => {
          devLog('[SETTINGS] teams-auth-complete received');
          setTeamsPhase('done');
          teamsStatusConnected = true;
          // The new token carries the freshly-granted scope set — refresh so
          // the presence banner disappears. Issue #3.0-P1/P2.
          refreshTeamsGrantedScopes();
        },
        onTeamsFailed: (payload) => {
          console.error('[SETTINGS] teams-auth-failed:', payload);
          setTeamsPhase('error', String(payload));
        }
      },
      [
        [
          'spotify-secret-conflict',
          (event) => {
            devLog('[SETTINGS] spotify-secret-conflict received:', event.payload);
            spotifySecretConflict = true;
          }
        ]
      ]
    );

    await loadConfig();
    localConfig = structuredClone($configStore);
    // Issue #538: the lexicon editor is a textarea (one entry per line), so the
    // stored list is projected into it here — after every load, including the
    // post-save adoption below.
    extraWordsText = localConfig.teams.profanity_extra_words.join('\n');

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
    if (teardownAuth) void teardownAuth();
  });

  async function handleSave() {
    isSaving = true;
    saveMessage = '';
    try {
      // Issue #538: mirror `clamp_teams` before the payload leaves the
      // frontend, so the store/UI never claims an entry the backend dropped.
      localConfig.teams.profanity_extra_words = extraWordsClamp.clamped;
      // `localConfig` is a Svelte 5 `$state` proxy; `structuredClone` in
      // `toSavePayload` rejects proxies with a DataCloneError, aborting the
      // save before IPC (#285). Snapshot to a plain object first.
      // Issue #297: adopt the value the backend actually persisted, so the
      // form shows the clamped numbers rather than the raw input.
      localConfig = await saveConfig($state.snapshot(localConfig));
      extraWordsText = localConfig.teams.profanity_extra_words.join('\n');
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

  // ── 4.7.0 (S5): logging + backup cards ────────────────────────────────
  //
  // Rust mirrors of `config.rs::clamp_logging` (1..=500 MB, 1..=20 files), so
  // a typed value shows the value the backend will actually store.
  const LOG_MAX_FILE_SIZE_MB = { min: 1, max: 500 } as const;
  const LOG_KEEP_FILES = { min: 1, max: 20 } as const;
  /**
   * The wire values `config.rs::apply_log_level` matches (case-insensitively).
   * Shown verbatim: they are identifiers a log reader is looking for, like the
   * locale endonyms in the appearance card, not prose to translate.
   */
  const LOG_LEVELS = ['Off', 'Error', 'Warn', 'Info', 'Debug', 'Trace'] as const;

  let backupBusy = $state(false);
  let backupMessage = $state('');

  /** `{ path, config }` — the Rust `ImportOutcome` (commands/config.rs). */
  type ImportOutcome = { path: string; config: AppConfig };

  function backupError(e: unknown): string {
    return String((e as Error)?.message ?? e).slice(0, 180);
  }

  async function exportConfig() {
    if (backupBusy) return;
    backupBusy = true;
    backupMessage = '';
    try {
      const path = await invoke<string | null>('export_config', {
        title: t('settings.backupExportDialogTitle')
      });
      // `null` = the dialog was dismissed: not a failure, and no message.
      if (path) backupMessage = t('settings.backupExported', { path });
    } catch (e) {
      console.error('[SETTINGS] export_config failed:', e);
      backupMessage = t('settings.backupError', { error: backupError(e) });
    } finally {
      backupBusy = false;
    }
  }

  async function importConfig() {
    if (backupBusy) return;
    backupBusy = true;
    backupMessage = '';
    try {
      // Both the picker and the overwrite confirmation live in the
      // `import_config` command (Rust), so the same dialog appears in the main
      // window and in a popped-out Settings pane — the JS dialog plugin is
      // ACL-gated per window, and granting it to detached panes would hand them
      // the file dialogs too. Only the *copy* is localized here: Rust has no
      // dictionary, so the title, the body and both button labels arrive as
      // arguments and every one of them goes through `t()`.
      const outcome = await invoke<ImportOutcome | null>('import_config', {
        title: t('settings.backupImportDialogTitle'),
        confirmBody: t('settings.backupConfirmOverwrite'),
        confirmOk: t('common.yes'),
        confirmCancel: t('common.no')
      });
      if (!outcome) return;
      // Adopt what is now on disk (the #297 invariant) — including the
      // lexicon textarea, which is a projection of the stored list.
      localConfig = await loadConfig();
      extraWordsText = localConfig.teams.profanity_extra_words.join('\n');
      saveMessage = '';
      backupMessage = t('settings.backupImported', { path: outcome.path });
    } catch (e) {
      console.error('[SETTINGS] import_config failed:', e);
      // Rust refuses an import that carries a plaintext client_secret and
      // names the offending path; surface that text rather than a generic
      // failure, since it is the only way the user learns why.
      backupMessage = t('settings.backupError', { error: backupError(e) });
    } finally {
      backupBusy = false;
    }
  }

  async function reconnectSpotify() {
    if (spotifyAuthWaiting || !localConfig.spotify.client_id) return;
    // #550: `reconnect_spotify_session` emits `spotify-reconnect-required`,
    // which the always-mounted main-window listener answers with
    // `currentView.set('settings')`. From a popped-out pane that opens a
    // *second* Settings view beside this one, so hand the navigation back to
    // the main window and close this one instead — the same route
    // goToOnboarding takes.
    if (detached) {
      forwardToMain('settings');
      return;
    }
    // #421: fresh entry clears this flow's stale phase only; never the sibling's.
    resetSpotifyAuthFlow();
    setSpotifyPhase('waiting');
    try {
      // #554: re-authorize only. `reconnect_spotify` also cleared the stored
      // tokens and the keychain client_secret, pushing a returning user back
      // through the onboarding wizard to re-enter their Client ID/Secret.
      await invoke('reconnect_spotify_session', {
        clientId: localConfig.spotify.client_id,
        redirectUri: 'presencejam://callback'
      });
    } catch (e) {
      console.error('[SETTINGS] reconnect_spotify_session failed:', e);
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
    // #693: a reconnect is the retry path this banner offers, so the warning
    // clears as soon as one is initiated — a persist failure on the new
    // sign-in re-raises it from the backend event. (The backend emits the
    // warning *before* `teams-auth-complete`, so the completion handler must
    // not clear it: that would erase the fault it just reported.)
    clearAuthPersistWarning();
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

  async function toggleNotificationClass(cls: NotificationClass, e: Event) {
    const target = e.currentTarget as HTMLInputElement;
    const applied = await setNotificationPreference(cls, target.checked);
    if (!applied) {
      // The store kept the class off, so reset the DOM property this click
      // already flipped.
      target.checked = false;
      notificationsMessage = t('settings.notificationsDenied');
      return;
    }
    notificationsMessage = '';
    // The form owns a full-config copy; a later "Save" must not write a stale
    // notifications section back over the toggle that was just persisted.
    localConfig.notifications = { ...$notificationPreferences };
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
          <!-- #560: three states, not two. `client_secret_set` is the
               `Present`-only projection, so branching on it alone told a user
               whose keyring was locked that nothing was configured and
               pointed them at onboarding to re-enter a secret that is still
               stored. -->
          {#if spotifySecretState === 'present'}
            {t('settings.secretStoredHint')}
          {:else if spotifySecretState === 'unavailable'}
            {t('settings.secretKeychainUnavailable')}
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
      {#if $presence.authPersistWarning}
        <!-- #693: dismissible as well as retryable — the cause can be one the
             user cannot fix in-session (a permanently locked keychain, a
             read-only disk), and a banner that only clears after a
             *successful* reconnect would be undismissable there. -->
        <div class="persist-banner" role="alert">
          <span class="hint">{t('settings.teamsPersistWarning')}</span>
          <button type="button" class="btn-link" onclick={reconnectTeams} disabled={teamsAuthWaiting}>{t('common.reconnect')}</button>
          <button type="button" class="btn-link dismiss" onclick={clearAuthPersistWarning}>{t('common.dismiss')}</button>
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
      <!-- Findings #635/#637: the manual-status policy (ON by default) and the
           opt-in out-of-office gate, in the card the meeting/call gate lives in. -->
      <div class="toggle-row">
        <label for="respect-manual-status">{t('settings.respectManualStatusLabel')}</label>
        <input
          id="respect-manual-status"
          type="checkbox"
          bind:checked={localConfig.teams.respect_manual_status}
        />
      </div>
      <p class="hint">
        {t('settings.respectManualStatusHint')}
      </p>
      <div class="toggle-row">
        <label for="gate-out-of-office">{t('settings.gateOutOfOfficeLabel')}</label>
        <input
          id="gate-out-of-office"
          type="checkbox"
          bind:checked={localConfig.teams.gate_when_out_of_office}
        />
      </div>
      <p class="hint">
        {t('settings.gateOutOfOfficeHint')}
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
        <p class="hint">{t('rules.quietWindowHint')}</p>
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
              <!-- S4 (issue #672): the same `00:00`-means-midnight mapping the
                   track-rule window uses, so the picker can never save a
                   silently inert `00:00–00:00` quiet window (Rust clamps the end
                   to 1439 for the comparison, and 1440 is the end of the day
                   there too). -->
              <input
                type="time"
                value={minutesToTime(entry.end_minutes)}
                onchange={(e) => { entry.end_minutes = endMinutesFromTime((e.currentTarget as HTMLInputElement).value, entry.end_minutes); }}
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
            <div class="rule-row">
              <label class="rule-check">
                <input type="checkbox" bind:checked={entry.pause_polling} />
                <span>{t('rules.pausePollingLabel')}</span>
              </label>
            </div>
            <p class="hint">{t('rules.pausePollingHint')}</p>
            <div class="rule-row">
              <!-- Issue #538: the quiet-hours replacement status was config-only
                   until 4.6 — this is its editor. Finding #634: the same row
                   carries the rule's Teams presence action. -->
              <input
                type="text"
                bind:value={entry.replacement_status}
                maxlength={MAX_RULE_STATUS_CHARS}
                placeholder={t('rules.quietReplacementPlaceholder')}
                aria-label={t('rules.quietReplacementPlaceholder')}
              />
              <select
                value={presenceValue(entry.presence_availability, entry.presence_activity)}
                onchange={(e) => applyPresenceValue(entry, (e.currentTarget as HTMLSelectElement).value)}
                aria-label={t('rules.presenceLabel')}
              >
                <option value="">{t('rules.presenceNone')}</option>
                {#each PRESENCE_OPTIONS as option}
                  <option value={`${option.availability}|${option.activity}`}>{option.label}</option>
                {/each}
              </select>
            </div>
            {#if entry.replacement_status.length >= MAX_RULE_STATUS_CHARS}
              <p class="clamp-hint" role="status">
                {t('rules.replacementClampHint', { max: MAX_RULE_STATUS_CHARS })}
              </p>
            {/if}
            <p class="hint">{t('rules.presenceHint')}</p>
          </div>
        {/each}
        <button
          type="button"
          class="btn-secondary"
          onclick={() => { localConfig.status_rules.quiet_hours.push({ enabled: true, start_minutes: 1320, end_minutes: 420, days: [], replacement_status: '', presence_availability: '', presence_activity: '', pause_polling: false }); }}
        >{t('rules.addQuietHours')}</button>
      </div>
      <div class="form-group">
        <span class="form-label">{t('rules.trackRulesLabel')}</span>
        <p class="hint">{t('rules.trackRulesOrderHint')}</p>
        {#if localConfig.status_rules.track_rules.length === 0}
          <p class="hint">{t('rules.noTrackRules')}</p>
        {/if}
        {#each localConfig.status_rules.track_rules as rule, j}
          <div class="rule-row rule-col" role="group" aria-label={t('rules.trackRulesLabel')}>
            <div class="rule-row">
              <label class="rule-check">
                <input type="checkbox" bind:checked={rule.enabled} />
                <span>{t('rules.ruleEnabled')}</span>
              </label>
              <button
                type="button"
                class="btn-link"
                disabled={j === 0}
                aria-label={t('rules.moveRuleUp', { n: j + 1 })}
                onclick={() => moveRule(localConfig.status_rules.track_rules, j, -1)}
              >↑</button>
              <button
                type="button"
                class="btn-link"
                disabled={j === localConfig.status_rules.track_rules.length - 1}
                aria-label={t('rules.moveRuleDown', { n: j + 1 })}
                onclick={() => moveRule(localConfig.status_rules.track_rules, j, 1)}
              >↓</button>
              <button
                type="button"
                class="btn-link"
                onclick={() => { localConfig.status_rules.track_rules.splice(j, 1); }}
              >{t('rules.removeRule')}</button>
            </div>
            <div class="rule-row">
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
            </div>
            <div class="rule-row">
              <input
                type="text"
                bind:value={rule.replacement_status}
                maxlength={MAX_RULE_STATUS_CHARS}
                placeholder={t('rules.replacementPlaceholder')}
                aria-label={t('rules.replacementPlaceholder')}
              />
              <select
                value={presenceValue(rule.presence_availability, rule.presence_activity)}
                onchange={(e) => applyPresenceValue(rule, (e.currentTarget as HTMLSelectElement).value)}
                aria-label={t('rules.presenceLabel')}
              >
                <option value="">{t('rules.presenceNone')}</option>
                {#each PRESENCE_OPTIONS as option}
                  <option value={`${option.availability}|${option.activity}`}>{option.label}</option>
                {/each}
              </select>
            </div>
            {#if rule.replacement_status.length >= MAX_RULE_STATUS_CHARS}
              <p class="clamp-hint" role="status">
                {t('rules.replacementClampHint', { max: MAX_RULE_STATUS_CHARS })}
              </p>
            {/if}
            <!-- S4 (issue #672): the rule's own window, reusing the quiet-hours
                 time inputs and weekday picker verbatim. -->
            <div class="rule-row">
              <input
                type="time"
                value={minutesToTime(rule.start_minutes)}
                onchange={(e) => { rule.start_minutes = timeToMinutes((e.currentTarget as HTMLInputElement).value, rule.start_minutes); }}
                aria-label={t('rules.ruleStart')}
              />
              <span aria-hidden="true">–</span>
              <input
                type="time"
                value={minutesToTime(rule.end_minutes)}
                onchange={(e) => { rule.end_minutes = endMinutesFromTime((e.currentTarget as HTMLInputElement).value, rule.end_minutes); }}
                aria-label={t('rules.ruleEnd')}
              />
            </div>
            <div class="rule-row days-row" role="group" aria-label={t('rules.ruleDays')}>
              {#each [1, 2, 3, 4, 5, 6, 7] as day}
                <label class="rule-check day-check">
                  <input
                    type="checkbox"
                    checked={rule.days.includes(day)}
                    onchange={(e) => {
                      const on = (e.currentTarget as HTMLInputElement).checked;
                      rule.days = on
                        ? [...rule.days, day].sort()
                        : rule.days.filter((d) => d !== day);
                    }}
                  />
                  <span>{t(`rules.day${day}` as 'rules.day1')}</span>
                </label>
              {/each}
            </div>
            <p class="hint">{t('rules.presenceHint')}</p>
          </div>
        {/each}
        <button
          type="button"
          class="btn-secondary"
          onclick={() => { localConfig.status_rules.track_rules.push({ enabled: false, artist_substring: '', track_substring: '', replacement_status: '', presence_availability: '', presence_activity: '', days: [], start_minutes: 0, end_minutes: 1440 }); }}
        >{t('rules.addTrackRule')}</button>
      </div>
      <div class="form-group">
        <span class="form-label">{t('rules.manualStatusLabel')}</span>
        <p class="hint">{t('rules.manualStatusHint')}</p>
        <div class="rule-row">
          <input
            type="text"
            bind:value={localConfig.teams.paused_status_format}
            maxlength={MAX_RULE_STATUS_CHARS}
            placeholder={t('rules.pausedStatusPlaceholder')}
            aria-label={t('rules.pausedStatusPlaceholder')}
          />
          <input
            type="text"
            bind:value={localConfig.teams.stopped_status_format}
            maxlength={MAX_RULE_STATUS_CHARS}
            placeholder={t('rules.stoppedStatusPlaceholder')}
            aria-label={t('rules.stoppedStatusPlaceholder')}
          />
        </div>
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
      <!-- Issue #581: episodes use their own template, so a user editing the
           music template must know it does not apply to podcasts. -->
      <p class="hint">{t('settings.episodeFormatHint')}</p>
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
        <!-- Issue #538: `teams.profanity_extra_words` was config-only until
             4.6. The counter mirrors Rust's `clamp_teams` (64 entries × 32
             chars) so the truncation is never silent. -->
        <div class="form-group">
          <label for="profanity-extra-words">{t('settings.extraWordsLabel')}</label>
          <p class="hint">{t('settings.extraWordsHint')}</p>
          <textarea
            id="profanity-extra-words"
            rows="3"
            bind:value={extraWordsText}
            placeholder={t('settings.extraWordsPlaceholder')}
          ></textarea>
          {#if extraWordsClamp.active}
            <p class="clamp-hint" role="status">
              {t('settings.extraWordsClampHint', {
                max: extraWordsClamp.maxEntries,
                chars: extraWordsClamp.maxChars,
                kept: extraWordsClamp.kept
              })}
            </p>
          {/if}
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
      <!-- Issue #538: `polling.pause_backoff_max_seconds` was config-only
           until 4.6. `min`/`max` mirror Rust's `clamp_polling` (60..=3600) and
           the hint reports the effective value a typed value would land on. -->
      <div class="form-group">
        <label for="pause-backoff-max">
          {t('settings.pauseBackoffMaxLabel')}
        </label>
        <input
          id="pause-backoff-max"
          type="number"
          min={PAUSE_BACKOFF_MIN_SECONDS}
          max={PAUSE_BACKOFF_MAX_SECONDS}
          bind:value={localConfig.polling.pause_backoff_max_seconds}
        />
        {#if pauseBackoffClamp.active}
          <p class="clamp-hint" role="status">
            {t('settings.pauseBackoffClampHint', {
              min: PAUSE_BACKOFF_MIN_SECONDS,
              max: PAUSE_BACKOFF_MAX_SECONDS,
              effective: pauseBackoffClamp.effective
            })}
          </p>
        {/if}
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
      {#each NOTIFICATION_CLASSES as cls (cls)}
        <div class="toggle-row">
          <label for={`notifications-${cls}`}>{t(NOTIFICATION_LABELS[cls])}</label>
          <input
            id={`notifications-${cls}`}
            type="checkbox"
            checked={$notificationPreferences[cls]}
            onchange={(e) => toggleNotificationClass(cls, e)}
          />
        </div>
      {/each}
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
          onchange={(e) => {
            const next = (e.currentTarget as HTMLSelectElement).value as Locale;
            // 4.7.0 (issue #674): `config.locale` is the single source of
            // truth. The store applies the locale to this webview, persists
            // it and relabels the tray + native application menu; the draft is
            // kept in step so a language change alone never marks the form
            // dirty.
            localConfig.locale = next;
            void i18n.set(next);
          }}
        >
          <option value="en">English</option>
          <option value="de">Deutsch</option>
          <option value="fr">Français</option>
        </select>
        <p class="hint">{t('settings.languageHint')}</p>
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

    <!-- 4.7.0 (S5): log rotation. Rust owns the file target; this card is the
         only place `logging.*` is edited. `enabled`/`log_level` take effect
         immediately (config::apply_log_level, CfgDiag#4); size and retention
         are read once when the log plugin is built, i.e. at the next launch —
         the hint says so rather than implying an immediate effect. -->
    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionLogging')}</h2>
      </header>
      <div class="toggle-row">
        <label for="logging-enabled">{t('settings.loggingEnabledLabel')}</label>
        <input id="logging-enabled" type="checkbox" bind:checked={localConfig.logging.enabled} />
      </div>
      <div class="form-group">
        <label for="log-level">{t('settings.logLevelLabel')}</label>
        <select id="log-level" bind:value={localConfig.logging.log_level}>
          {#each LOG_LEVELS as level (level)}
            <option value={level}>{level}</option>
          {/each}
        </select>
      </div>
      <div class="row-2">
        <div class="form-group">
          <label for="log-max-size">{t('settings.logMaxSizeLabel')}</label>
          <input
            id="log-max-size"
            type="number"
            min={LOG_MAX_FILE_SIZE_MB.min}
            max={LOG_MAX_FILE_SIZE_MB.max}
            bind:value={localConfig.logging.max_file_size_mb}
          />
        </div>
        <div class="form-group">
          <label for="log-keep-files">{t('settings.logKeepFilesLabel')}</label>
          <input
            id="log-keep-files"
            type="number"
            min={LOG_KEEP_FILES.min}
            max={LOG_KEEP_FILES.max}
            bind:value={localConfig.logging.keep_files}
          />
        </div>
      </div>
      <p class="hint">{t('settings.logRotationHint')}</p>
      <button class="btn-secondary btn-full" onclick={openLogs}>
        {t('settings.openLogsFolder')}
      </button>
    </section>

    <!-- 4.7.0 (S5): backup. Both actions run in Rust, which owns the file
         dialogs and the resolved path; the export never carries the Spotify
         client secret (keychain-only) and the import refuses a document that
         does. -->
    <section class="card">
      <header class="section-header">
        <h2>{t('settings.sectionBackup')}</h2>
      </header>
      <p class="hint">{t('settings.backupHint')}</p>
      <div class="row-2">
        <button class="btn-secondary btn-full" onclick={exportConfig} disabled={backupBusy}>
          {t('settings.backupExport')}
        </button>
        <button class="btn-secondary btn-full" onclick={importConfig} disabled={backupBusy}>
          {t('settings.backupImport')}
        </button>
      </div>
      {#if backupMessage}
        <p class="hint" role="status">{backupMessage}</p>
      {/if}
    </section>

    <section class="actions">
      <button class="btn-full" onclick={handleSave} disabled={isSaving}>
        {isSaving ? t('settings.saving') : t('settings.saveChanges')}
      </button>
      {#if saveMessage}
        <p class="save-message" aria-live="polite">{saveMessage}</p>
      {/if}
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

  /* #693: the Teams session could not be persisted (locked keychain, full
     disk). Amber, like the dirty banner: the sign-in itself succeeded, so
     this is a warning the user can still act on by reconnecting. */
  .persist-banner {
    margin-top: var(--sp-3);
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--warning-soft);
    color: var(--warning);
    border-radius: var(--r-md);
  }
  .persist-banner .hint { margin: 0; color: inherit; }
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
