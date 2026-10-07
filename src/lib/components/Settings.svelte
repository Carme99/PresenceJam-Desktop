<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView, settingsDirty, pendingMenuNav, type View } from '$lib/stores/app';
  import { emitTo } from '@tauri-apps/api/event';
  // C7 multi-window detach: pop-out/pop-back controls.
  import { popOut, popIn } from '$lib/stores/detach';

  // When rendered in the detached `settings-detached` window, "Back" pops
  // the pane back into the main window (closes this one); the onboarding
  // redirect forwards the navigation to the main window first.
  let { detached = false }: { detached?: boolean } = $props();
  import RulesCard from './settings/RulesCard.svelte';
  import LoggingCard from './settings/LoggingCard.svelte';
  import BackupCard from './settings/BackupCard.svelte';
  import ShortcutsCard from './settings/ShortcutsCard.svelte';
  import SpotifyCard from './settings/SpotifyCard.svelte';
  import TeamsCard from './settings/TeamsCard.svelte';
  import PresenceCard from './settings/PresenceCard.svelte';
  import StatusFormatCard from './settings/StatusFormatCard.svelte';
  import PollingCard from './settings/PollingCard.svelte';
  import NotificationsCard from './settings/NotificationsCard.svelte';
  import AppearanceCard from './settings/AppearanceCard.svelte';
  import ProfilesCard from './settings/ProfilesCard.svelte';
  import UpdatesCard from './settings/UpdatesCard.svelte';
  import { shortcutReasonLabel, normalizeShortcutReason } from '$lib/utils/shortcuts';
  import { configStore, saveConfig, loadConfig, updateConfig, defaultConfig, clientSecretStateOf, DEFAULT_PROFANITY_PLACEHOLDER } from '$lib/stores/config';
  import type { AppConfig, SyncStatus } from '$lib/types';
  import { authFlow, setSpotifyPhase, setTeamsPhase, resetSpotifyAuthFlow, resetTeamsAuthFlow, teamsPollMutex, pollTeamsAuth } from '$lib/stores/authFlow.svelte';
  import DeviceCodeBox from './DeviceCodeBox.svelte';
  import { useAuthListeners } from '$lib/utils/useAuthListeners';
  import { pickReconnectProvider } from '$lib/utils/routeReconnect';
  import PageHeader from './PageHeader.svelte';
  import { t, i18n, type Locale, type TKey } from '$lib/i18n';
  import { theme, density } from '$lib/stores/theme';
  import {
    NOTIFICATION_CLASSES,
    notificationPreferences,
    setNotificationPreference,
    type NotificationClass
  } from '$lib/stores/notifications';
  import { presence, clearAuthPersistWarning } from '$lib/stores/presence';
  import { devLog } from '$lib/utils/dev';

  let localConfig = $state<AppConfig>(structuredClone($configStore));
  // #984: the follow-system checkbox mirrors the i18n store's own flag
  // (a resolution policy, not a language). A plain getter read would not
  // re-render on change, so a local copy is synced after each toggle.
  let followSystemChecked = $state(i18n.followSystem);
  let isConnected = $state(false);
  let teamsStatusConnected = $state(false);
  let isSaving = $state(false);
  let saveMessage = $state('');
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;

  // #890: the draft's dirty state is an explicit flag. It used to be a
  // `$derived` that serialised the whole document twice — `localConfig` against
  // `$configStore`, status rules, lexicon and all — and every write to the deep
  // proxy invalidated it, so one keystroke in any field paid two full
  // `JSON.stringify` passes on the UI thread. The listeners on the form root
  // below set the flag and the save/discard paths clear it, so nothing compares
  // serialised configs here any more — which is also why the BigInt→number
  // replacer that comparison needed is gone with it.
  let isDirty = $state(false);

  /** #890: the draft now differs from the saved config. #817: published to the shared store so the root navigate listener can park a menu navigation. */
  function markDirty() {
    isDirty = true;
    settingsDirty.set(true);
  }
  // #750: per-card refs. Rules owns its pending removal (Undo/clear on
  // save/discard); Shortcuts owns its registration status (re-register
  // after save). Both sync through `markDirty` / `isDirty` below.
  let rulesCard: { commitRemoval: () => void; clearRemoval: () => void } | null =
    $state(null);
  let shortcutsCard: {
    pendingRejection: () => { reason: import('$lib/types').ShortcutReason } | null;
    refreshAfterSave: () => void;
  } | null = $state(null);

  /**
   * #890: an edit anywhere in the form marks the draft dirty. `input` and
   * `change` both bubble, so one pair of listeners on the form root covers
   * every control — including any added later — instead of re-serialising the
   * config to find out.
   *
   * A control opts out with `data-no-draft`: the settings that apply themselves
   * immediately through their own store, and the controls that only drive the
   * reconnect flow. Anything that edits `localConfig` or the lexicon must not
   * carry it.
   */
  function onDraftEdit(event: Event) {
    const target = event.target;
    if (target instanceof Element && target.closest('[data-no-draft]') !== null) return;
    markDirty();
  }

  // C9: effective polling bounds, mirroring Rust `clamp_polling`
  // (`config::clamp_polling`): minimum clamps to [5, 30] first, then
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
    // Issues #872/#873: the OS-level presentation gate and the
    // desktop-idle gate reset with the card, exactly like the OOO
    // opt-in — an untouched config is identical byte-for-byte to 4.7.
    localConfig.teams.gate_when_presenting = defaultConfig.teams.gate_when_presenting;
    localConfig.teams.idle_away_after_seconds = defaultConfig.teams.idle_away_after_seconds;
    markDirty();
  }
  function resetStatusFormatDefaults() {
    localConfig.teams.status_format = defaultConfig.teams.status_format;
    localConfig.teams.profanity_filter = defaultConfig.teams.profanity_filter;
    // Keep the shipped English sentinel in the draft. The input displays the
    // localized default below, while Rust receives provenance it can localize
    // again after a later language switch.
    localConfig.teams.profanity_placeholder = DEFAULT_PROFANITY_PLACEHOLDER;
    // Issue #538: the custom lexicon belongs to this card too.
    localConfig.teams.profanity_extra_words = [...defaultConfig.teams.profanity_extra_words];
    extraWordsText = '';
    markDirty();
  }

  function isDefaultProfanityPlaceholder(value: string): boolean {
    return value.trim() === '' || value === DEFAULT_PROFANITY_PLACEHOLDER;
  }

  let profanityPlaceholderDisplay = $derived(
    isDefaultProfanityPlaceholder(localConfig.teams.profanity_placeholder)
      ? t('settings.placeholderTextPlaceholder')
      : localConfig.teams.profanity_placeholder
  );
  // Issue #432: reset the rules section to its (empty) default. Rules are
  // additive with serde defaults, so a default section is always valid.
  function resetRulesDefaults() {
    localConfig.status_rules = structuredClone(defaultConfig.status_rules);
    // S4 (issue #672): the card also renders the two manual-status texts, so
    // Reset must not leave those editors showing a stale value.
    localConfig.teams.paused_status_format = defaultConfig.teams.paused_status_format;
    localConfig.teams.stopped_status_format = defaultConfig.teams.stopped_status_format;
    // #981: the reset replaced the lists, so a pending removal would splice
    // a stale entry back into the fresh defaults on Undo.
    rulesCard?.clearRemoval();
    markDirty();
  }
  // Issue #869: name for a freshly-added profile. The Rust side dedupes
  // again on load, so a concurrent edit cannot wedge the form — this is
  // only the prefix the new-row picker suggests. `Profile` is the prefix
  // the user can rename, and the trailing number is just a uniqueness
  // hint until they do.
  function defaultProfileName(n: number): string {
    return `Profile ${n}`;
  }
  function resetPollingDefaults() {
    localConfig.polling = structuredClone(defaultConfig.polling);
    markDirty();
  }

  // ── 4.6 findings #634/#635/#637 + issue #538 consumption sites ──────────
  //
  // Frontend mirrors of the Rust clamps, so a typed/pasted value shows the
  // value the backend will actually store (`config.rs::clamp_rules`,
  // `::clamp_teams`, `::clamp_polling`) instead of silently differing.
  const EXTRA_WORDS_MAX_ENTRIES = 64;
  const EXTRA_WORDS_MAX_CHARS = 32;
  const PAUSE_BACKOFF_MIN_SECONDS = 60;
  const PAUSE_BACKOFF_MAX_SECONDS = 3600;
  // Issue #869: presence-profile bounds mirror `clamp_presence_profiles`.
  // The Rust side is the source of truth, so these constants exist only to
  // give the input its `maxlength` / `max` attribute. A user typing past
  // either is still accepted by Rust, but the form lets them see the
  // effective value the backend stored rather than the raw keystrokes.
  const MAX_PROFILE_ID_CHARS = 32;
  const MAX_PROFILE_IDLE_SECONDS = 86400;
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
    theme.set('system');
    density.set('comfortable');
    localConfig.locale = 'en';
    void i18n.set('en');
    localConfig.autostart = defaultConfig.autostart;
    markDirty();
  }

  // #552: a radiogroup must own `role="radio"`/`aria-checked` children with a
  // roving tabindex and arrow-key navigation. The cards declared
  // `aria-pressed`, which assistive tech ignores inside a radiogroup and
  // which carries no single-selection contract at all.
  let themeDarkButton: HTMLButtonElement | undefined = $state();
  let themeLightButton: HTMLButtonElement | undefined = $state();
  let themeSystemButton: HTMLButtonElement | undefined = $state();

  // #680: `system` joins the radiogroup, so the arrow-key walk has to cycle
  // through three cards instead of toggling two.
  const THEME_OPTIONS = ['dark', 'light', 'system'] as const;
  type ThemeOption = (typeof THEME_OPTIONS)[number];

  function themeRadioKeydown(e: KeyboardEvent, current: ThemeOption) {
    const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
    const isPrev = e.key === 'ArrowLeft' || e.key === 'ArrowUp';
    if (!isNext && !isPrev) return;
    e.preventDefault();
    const step = isNext ? 1 : THEME_OPTIONS.length - 1;
    const next = THEME_OPTIONS[(THEME_OPTIONS.indexOf(current) + step) % THEME_OPTIONS.length];
    theme.set(next);
    // Selection follows focus, and the roving tabindex moves with it.
    const buttons: Record<ThemeOption, HTMLButtonElement | undefined> = {
      dark: themeDarkButton,
      light: themeLightButton,
      system: themeSystemButton
    };
    buttons[next]?.focus();
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
  // #675: the form's `localConfig` is snapshotted once, but the notification
  // classes are immediate-apply and shared, so a toggle made in the *other*
  // Settings view (a popped-out pane runs beside this one) — or in the main
  // window — must reach this form's copy. Without this, `handleSave` would
  // write the stale section back over the choice the user just made. Only the
  // notifications section is followed: every other field here is a pending
  // edit that Save owns.
  $effect(() => {
    const next = $configStore.notifications;
    if (NOTIFICATION_CLASSES.some((cls) => localConfig.notifications[cls] !== next[cls])) {
      localConfig.notifications = { ...next };
    }
  });
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
  // `null` means the backend could not determine the granted scopes (an
  // undecodable token) — that must not read as a missing permission, since
  // reconnecting cannot fix a decode failure (issue #973).
  let grantedScopes = $state<string[] | null>(null);
  let playbackScopeMissing = $derived(
    isConnected &&
      grantedScopes !== null &&
      !grantedScopes.includes('user-modify-playback-state')
  );
  // Issue #376: set when the setup-path migration finds a legacy plaintext
  // in config.json that differs from the keychain entry. Issue #813: the
  // one-time `spotify-secret-conflict` event fires before any webview has
  // mounted, so the flag is seeded from `get_sync_status`'s replayable
  // `spotify_secret_conflict` field in `onMount` (the event listener below
  // stays as a belt-and-braces path). The banner below prompts a Spotify
  // reconnect; the plaintext is left untouched until then.
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
      grantedScopes =
        (await invoke<string[] | null>('get_spotify_granted_scopes')) ?? null;
    } catch (e) {
      console.error('[SETTINGS] get_spotify_granted_scopes failed:', e);
      grantedScopes = null;
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
      teamsGrantedScopes = (await invoke<string[]>('get_teams_granted_scopes')) ?? [];
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
    const locale = i18n.locale;
    const profane_sample = previewProfaneSample;
    // Issue #538: the preview must run the user's own lexicon too, otherwise a
    // word they just added shows no effect until the next real track.
    const extra_words = extraWordsClamp.clamped;
    if (previewDebounce) clearTimeout(previewDebounce);
    const my = ++previewSeq;
    previewDebounce = setTimeout(async () => {
      try {
        const v = await invoke<string>('preview_status', {
          format,
          filter_enabled,
          placeholder,
          profane_sample,
          extra_words,
          locale
        });
        if (my !== previewSeq || locale !== i18n.locale) return;
        // #748: an identical sample is not a new one — never rewrite the node.
        if (v !== previewText) previewText = v;
      } catch (e) {
        if (my !== previewSeq || locale !== i18n.locale) return;
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
      // Issue #813: replay the startup migration conflict. The one-shot
      // `spotify-secret-conflict` event fires from the setup hook before any
      // webview has mounted, so the extra listener above can never observe
      // it — the persisted `spotify_secret_conflict` field is what raises
      // the banner for a conflicting legacy plaintext on disk. (Older
      // backends omit the field; `?? false` keeps the banner hidden there.)
      spotifySecretConflict = syncStatus.spotify_secret_conflict ?? false;
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
    // #817: Settings owns the shared draft state — a stale `true` after an
    // unguarded unmount would park every later menu navigation.
    settingsDirty.set(false);
    pendingMenuNav.set(null);
    if (teardownAuth) void teardownAuth();
  });

  async function handleSave() {
    isSaving = true;
    saveMessage = '';
    try {
      // Issue #538: mirror `clamp_teams` before the payload leaves the
      // frontend, so the store/UI never claims an entry the backend dropped.
      // 4.7.0 (issue #676): a combination the backend rejects must neither be
      // saved nor registered. The card validates on every edit, so the reason
      // is already recorded — read it here rather than issuing IPC in the save
      // path, which nothing else in this handler does.
      const rejected = shortcutsCard?.pendingRejection() ?? null;
      if (rejected !== null) {
        // Issue #968: render the localized label for the typed reason, not
        // the raw English string the validator used to splice in.
        saveMessage = t('settings.shortcutRejected', {
          reason: shortcutReasonLabel(rejected.reason)
        });
        return;
      }
      localConfig.teams.profanity_extra_words = extraWordsClamp.clamped;
      // #675: the notification classes live in the shared store and are not
      // form-edited — the checkboxes read it directly — so the authoritative
      // value goes into the payload here, next to the clamped-lexicon precedent
      // above. The `$effect` keeps the form visibly in step, but it cannot cover
      // the mount race: `onMount`'s `localConfig = <loaded cfg>` can land
      // *after* a sibling window's mirror convergence, re-staling this section
      // without `configStore` changing again.
      localConfig.notifications = { ...$configStore.notifications };
      // `localConfig` is a Svelte 5 `$state` proxy; `structuredClone` in
      // `toSavePayload` rejects proxies with a DataCloneError, aborting the
      // save before IPC (#285). Snapshot to a plain object first.
      // Issue #297: adopt the value the backend actually persisted, so the
      // form shows the clamped numbers rather than the raw input.
      localConfig = await saveConfig($state.snapshot(localConfig));
      extraWordsText = localConfig.teams.profanity_extra_words.join('\n');
      isDirty = false;
      settingsDirty.set(false);
      // #981: the removal is committed now, so there is nothing left to undo.
      rulesCard?.commitRemoval();
      saveMessage = t('settings.saved');
      if (saveTimeout) clearTimeout(saveTimeout);
      saveTimeout = setTimeout(() => saveMessage = '', 2000);
    } catch (e) { const msg = String((e as Error)?.message ?? e).slice(0, 180); saveMessage = msg || t('settings.failedToSave'); console.error('[SETTINGS] handleSave failed:', e); }
    finally { isSaving = false; }
    // 4.7.0 (issue #676): re-register from the config the backend just
    // persisted — this may be the first save after a capture released the
    // grabs. Fire-and-forget, and *after* the `finally`: awaiting it here
    // would hold `isSaving` (and so the Save button's `disabled`) open past
    // the store write, which is not this card's business. A rejected slot
    // returned above, so nothing is re-registered for a save that never
    // happened; both save paths (Save and "Save & leave") go through here.
    shortcutsCard?.refreshAfterSave();
  }
  // #750: BackupCard owns the dialogs and the status line; the parent only
  // re-snapshots the draft from what is now on disk (the #297 invariant).
  async function handleBackupImported() {
    localConfig = await loadConfig();
    extraWordsText = localConfig.teams.profanity_extra_words.join('\n');
    isDirty = false;
    settingsDirty.set(false);
    // #981: the imported document replaced the draft, so the pending
    // removal refers to a row that no longer exists.
    rulesCard?.clearRemoval();
    saveMessage = '';
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
    // Issue #932 / #693: the reconnect is the retry path this banner
    // offers, so the warning clears as soon as one is initiated — a persist
    // failure on the new sign-in re-raises it from the backend event.
    // (The backend emits the warning *before* `spotify-auth-complete`, so
    // the completion handler must not clear it: that would erase the
    // fault it just reported.)
    clearAuthPersistWarning();
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

  // ── #964: the waiting-state escape hatch ────────────────────────────────
  //
  // The poller's `spotify-reconnect-required` event lands the user on this
  // pane with the flow already waiting, so this card is where a lost browser
  // tab strands them. Reconnect has offered the two ways out since #558; this
  // is the same pair, reusing its copy.
  let spotifyManualUrl = $state('');
  let manualSubmitBusy = $state(false);
  let manualUrlError = $state('');

  // `reconnectSpotify` refuses a restart while the phase is `waiting`, so the
  // phase is cleared first and this is not a nested call for its own sake.
  async function restartSpotifySignIn() {
    resetSpotifyAuthFlow();
    await reconnectSpotify();
  }

  /** Extract `code`/`state` from a pasted Spotify redirect URL. */
  function extractCodeFromUrl(url: string): { code: string; state: string } | null {
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      if (!code) return null;
      // A missing `state` still passes (empty string) — the backend rejects it,
      // mirroring the deep-link CSRF check (#162).
      return { code, state: parsed.searchParams.get('state') ?? '' };
    } catch {
      return null;
    }
  }

  /** Complete the flow from a pasted redirect URL (the #385 fallback). */
  async function submitManualUrl() {
    if (manualSubmitBusy) return;
    const extracted = extractCodeFromUrl(spotifyManualUrl);
    if (!extracted) {
      manualUrlError = t('validation.noCodeInUrl');
      return;
    }
    manualSubmitBusy = true;
    manualUrlError = '';
    try {
      await invoke('complete_spotify_auth_manual', {
        code: extracted.code,
        oauthState: extracted.state
      });
      setSpotifyPhase('done');
    } catch (e) {
      console.error('[SETTINGS] complete_spotify_auth_manual failed:', e);
      manualUrlError = String(e);
      setSpotifyPhase('error', String(e));
    } finally {
      manualSubmitBusy = false;
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
  /**
   * #932 (rework): the auth-persist banner's reconnect action, label and
   * "in flight" disabled state all route off the `provider` discriminator.
   * Centralising the routing here lets the JSX pick `{reconnect.label}`,
   * `{reconnect.handler}` and `{reconnect.waiting}` without re-deriving
   * the same ternary three times in the template, and lets the unit test
   * assert the routing without re-implementing the ternary in JSX. The
   * pure routing decision lives in `src/lib/utils/routeReconnect.ts`
   * (`pickReconnectProvider`) so a Vitest spec can exercise it without
   * the component harness — the commit message on `e833931` claimed such
   * a test existed but no spec asserted the routing. An unknown value
   * falls back to the Spotify reconnect (the Spotify banner is the newer
   * of the two, #932 B1) so a future backend payload cannot crash the banner.
   */
  function routeReconnect(provider: 'teams' | 'spotify' | string): {
    label: string;
    handler: () => Promise<void>;
    waiting: boolean;
  } {
    const target = pickReconnectProvider(provider);
    if (target === 'teams') {
      return {
        label: t('settings.sectionTeams'),
        handler: reconnectTeams,
        waiting: teamsAuthWaiting
      };
    }
    return {
      label: t('settings.sectionSpotify'),
      handler: reconnectSpotify,
      waiting: spotifyAuthWaiting
    };
  }
  /**
   * #785: the shared poll — the #396 mutex, the #429 expiry guard, the phase
   * transitions and the #933/#978 superseded-flow guard all live in the store
   * now — plus this pane's own post-success state. The callback runs only on a
   * real success, so a skipped or failed poll cannot mark Teams connected here.
   */
  async function checkTeamsSignIn() {
    await pollTeamsAuth(() => {
      teamsStatusConnected = true;
    });
  }

  // #548: unsaved edits must gate navigation, not merely warn. `localConfig`
  // is the only copy of them — Settings is the sole writer of config.json —
  // so Back and "Run onboarding" park their target here while the banner
  // asks for a choice, instead of silently dropping queued rule edits, a
  // changed status template and changed polling bounds.
  // #817: menu-driven `navigate` events land in `+page.svelte`, which cannot
  // reach this component's locals — so `navigateTo` parks the target in the
  // shared `pendingMenuNav` store and the effect below mirrors it into the
  // same banner choice that resolves the in-component targets.
  // `back` is the in-component Back button (→ dashboard); every other value
  // is a real view, including a parked menu `navigate` target.
  type PendingNav = View | 'back';
  let pendingNav = $state<PendingNav | null>(null);
  // #817: mirror of the parked menu target published by `+page.svelte`'s
  // navigate listener (same choice of banner buttons resolves it). Kept as
  // an effect — not folded into `pendingNav` reads — so the banner condition
  // stays a single local and the store remains the cross-component channel.
  $effect(() => {
    const parked = $pendingMenuNav;
    if (parked !== null && pendingNav === null) pendingNav = parked;
  });

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

  /** #548: run the navigation a confirmed Save / Discard asked for. #817: a parked menu target navigates directly; `back` keeps the old Back path. */
  function leaveSettings(target: PendingNav) {
    if (target === 'back') {
      performBack();
      return;
    }
    if (target === 'onboarding') {
      performOnboarding();
      return;
    }
    currentView.set(target);
  }

  async function saveAndLeave() {
    await handleSave();
    // A failed save keeps `isDirty` set and reports the error through
    // `saveMessage`; stay on the form rather than navigating away from it.
    if (isDirty) return;
    // #817: the parked target may live only in the shared store (menu
    // navigation mirrored into `pendingNav` by the effect above). Prefer the
    // local, fall back to the store, and always clear both together.
    const target = pendingNav ?? $pendingMenuNav;
    pendingNav = null;
    pendingMenuNav.set(null);
    if (target) leaveSettings(target);
  }

  function discardAndLeave() {
    isDirty = false;
    settingsDirty.set(false);
    // #981: the draft is being abandoned, so its pending removal goes with it.
    rulesCard?.clearRemoval();
    const target = pendingNav ?? $pendingMenuNav;
    pendingNav = null;
    pendingMenuNav.set(null);
    if (target) leaveSettings(target);
  }

  /**
   * #966: put the draft back to the last saved configuration without leaving
   * the form. The dirty banner's Discard used to be unreachable unless a
   * navigation was blocked, and the per-card Reset buttons restore shipped
   * defaults rather than what is stored — so before this, a half-finished
   * edit could not be abandoned at all except by saving it. This re-snapshots
   * `$configStore` exactly the way the initial load does, so the visible draft
   * is the stored document again.
   */
  function revertChanges() {
    localConfig = structuredClone($configStore);
    // Issue #538: the lexicon textarea is a separate draft buffer, not a bind
    // on `localConfig`, so it needs the same re-snapshot the load path does.
    extraWordsText = localConfig.teams.profanity_extra_words.join('\n');
    isDirty = false;
    settingsDirty.set(false);
    // #981: the draft the pending removal belonged to is gone with it.
    rulesCard?.clearRemoval();
    saveMessage = '';
  }

  function stayHere() {
    pendingNav = null;
    pendingMenuNav.set(null);
  }
</script>

<div class="settings" oninput={onDraftEdit} onchange={onDraftEdit}>
  <PageHeader title={t('settings.title')} onBack={goBack}
    backLabel={detached ? t('settings.popBackIn') : t('common.back')}
    onAction={detached ? undefined : handlePopOut}
    actionTitle={detached ? '' : t('settings.popOutActionTitle')} />
  {#if isDirty}
    <!-- #966: the banner is the commit point. It used to carry actions only
         while a navigation was parked, so the ordinary case was a status line
         with no way to act on it and the sole Save sat at the end of a
         twelve-card form. Both variants live here now: the pendingNav labels
         differ because those actions navigate as well as commit or discard. -->
    <div class="dirty-banner" role="status">
      <span>{t('settings.unsavedChanges')}</span>
      <div class="dirty-actions">
        {#if pendingNav}
          <button type="button" class="btn-secondary" onclick={saveAndLeave} disabled={isSaving}>
            {isSaving ? t('settings.saving') : t('settings.saveAndLeave')}
          </button>
          <button type="button" class="btn-link" onclick={discardAndLeave}>{t('settings.discardChanges')}</button>
          <button type="button" class="btn-link" onclick={stayHere}>{t('settings.stayHere')}</button>
        {:else}
          <button type="button" class="btn-secondary" onclick={handleSave} disabled={isSaving}>
            {isSaving ? t('settings.saving') : t('settings.saveChanges')}
          </button>
          <button type="button" class="btn-link" onclick={revertChanges}>{t('settings.revertChanges')}</button>
        {/if}
      </div>
    </div>
  {/if}

  <!-- #742: the skip link's target (main window only). It used to sit on
       `.app-container` in +page.svelte, which wraps this view *and* the
       PageHeader above it, so "Skip to main content" landed on the very
       chrome the link promises to bypass. `.sections` is the first region
       below the header, so one Tab from here reaches the body's first
       control. `tabindex="-1"` keeps the target focusable without putting it
       in the tab order. The id is main-window-only: a detached pane mounts
       this view inside the detached route's own `#main-content` (#743), so
       carrying it here too would duplicate the id in that document. Only one
       view is mounted at a time, so the id stays unique per document. -->
  <div class="sections" id={detached ? undefined : 'main-content'} tabindex="-1">
    <SpotifyCard
      bind:spotify={localConfig.spotify}
      {isConnected}
      waiting={spotifyAuthWaiting}
      {playbackScopeMissing}
      secretConflict={spotifySecretConflict}
      {detached}
      onReconnect={reconnectSpotify}
      onGoToOnboarding={goToOnboarding}
      onRestartSignIn={reconnectSpotify}
      onForwardToMain={() => forwardToMain('settings')}
    />
    <TeamsCard
      teamsConnected={teamsStatusConnected}
      waiting={teamsAuthWaiting}
      remainingMs={teamsRemainingMs}
      codeExpired={teamsCodeExpired}
      scopesMissing={teamsScopesMissing}
      onReconnect={reconnectTeams}
      onReconnectSpotify={reconnectSpotify}
      onCheckNow={checkTeamsSignIn}
    />
    <PresenceCard bind:teams={localConfig.teams} onreset={resetPresenceDefaults} />
    <RulesCard
      bind:statusRules={localConfig.status_rules}
      bind:pausedStatusFormat={localConfig.teams.paused_status_format}
      bind:stoppedStatusFormat={localConfig.teams.stopped_status_format}
      {isDirty}
      onreset={resetRulesDefaults}
      onchange={markDirty}
      bind:this={rulesCard}
    />
    <ProfilesCard
      bind:presenceProfiles={localConfig.presence_profiles}
      bind:activeProfile={localConfig.active_profile}
      teams={localConfig.teams}
      bind:saveMessage
    />
    <StatusFormatCard
      bind:teams={localConfig.teams}
      {extraWordsText}
      placeholderDisplay={profanityPlaceholderDisplay}
      {previewText}
      {previewProfaneSample}
      onreset={resetStatusFormatDefaults}
      onExtraWordsInput={(v) => { extraWordsText = v; markDirty(); }}
      onProfaneSampleChange={(v) => { previewProfaneSample = v; }}
    />
    <PollingCard bind:polling={localConfig.polling} onreset={resetPollingDefaults} />
    <NotificationsCard />
    <AppearanceCard
      bind:locale={localConfig.locale}
      bind:autostart={localConfig.autostart}
      bind:saveMessage
      bind:followSystemChecked
      onreset={resetAppearanceDefaults}
      onchange={markDirty}
    />
    <LoggingCard bind:logging={localConfig.logging} />
    <BackupCard onimported={handleBackupImported} />
    <ShortcutsCard bind:shortcuts={localConfig.shortcuts} onchange={markDirty} bind:this={shortcutsCard} />

    <UpdatesCard bind:updates={localConfig.updates} />

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
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    margin-top: var(--sp-3);
  }

  .save-message {
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--success);
    font-weight: 600;
  }
</style>
